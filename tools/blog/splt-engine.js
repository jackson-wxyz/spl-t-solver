/* SPL-T rules in JavaScript, for replaying a solver game in the browser.
 *
 * A port of the "reference" path of src/engine.rs (itself a port of Josh Holder's Python
 * emulator): whole-board gravity in list order, a full scan for clusters after any fall.
 * The block list order is the engine's (a split shrinks the block in place and appends the
 * new half, exploded blocks are removed in order, refill blocks are appended), so a game
 * file's block indices mean the same thing here.
 *
 * tools/blog/verify.mjs checks this file against the Rust engine position by position.
 */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.SpltEngine = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';
  var W = 8, H = 16, EMPTY = 255;

  function Board() {
    this.blocks = [{ x: 0, y: 0, w: W, h: H, pts: 0 }];
    this.grid = new Uint8Array(W * H); // cell -> block index, EMPTY for a void
    this.moves = 0;
  }

  Board.prototype.clone = function () {
    var b = new Board();
    b.blocks = this.blocks.map(function (k) { return { x: k.x, y: k.y, w: k.w, h: k.h, pts: k.pts }; });
    b.grid = new Uint8Array(this.grid);
    b.moves = this.moves;
    return b;
  };

  /* The first split is horizontal (it cuts a block into a top and a bottom half). */
  Board.prototype.horizontalNext = function () { return this.moves % 2 === 0; };

  Board.prototype.canSplit = function (i) {
    var b = this.blocks[i];
    return !!b && b.pts === 0 && (this.horizontalNext() ? b.h > 1 : b.w > 1);
  };

  Board.prototype.legalMoves = function () {
    var out = [];
    for (var i = 0; i < this.blocks.length; i++) if (this.canSplit(i)) out.push(i);
    return out;
  };

  function paint(grid, b, v) {
    for (var y = b.y; y < b.y + b.h; y++) for (var x = b.x; x < b.x + b.w; x++) grid[y * W + x] = v;
  }

  Board.prototype.repaint = function () {
    this.grid.fill(EMPTY);
    for (var i = 0; i < this.blocks.length; i++) paint(this.grid, this.blocks[i], i);
  };

  /* Indices of the four identical free blocks in a 2x2 arrangement whose top-left member
   * has its corner at (tlx, tly), or null. */
  Board.prototype.groupAt = function (tlx, tly, w, h) {
    if (tlx < 0 || tly < 0 || tlx + 2 * w > W || tly + 2 * h > H) return null;
    var out = [], offs = [[0, 0], [w, 0], [0, h], [w, h]];
    for (var k = 0; k < 4; k++) {
      var cx = tlx + offs[k][0], cy = tly + offs[k][1];
      var id = this.grid[cy * W + cx];
      if (id === EMPTY) return null;
      var b = this.blocks[id];
      if (b.x !== cx || b.y !== cy || b.w !== w || b.h !== h || b.pts !== 0) return null;
      out.push(id);
    }
    return out;
  };

  Board.prototype.markGroupsContaining = function (c, mark) {
    var b = this.blocks[c];
    for (var ry = 0; ry < 2; ry++) for (var rx = 0; rx < 2; rx++) {
      var ids = this.groupAt(b.x - rx * b.w, b.y - ry * b.h, b.w, b.h);
      if (ids) for (var k = 0; k < 4; k++) mark[ids[k]] = true;
    }
  };

  Board.prototype.markAllGroups = function (mark) {
    for (var i = 0; i < this.blocks.length; i++) {
      var b = this.blocks[i];
      if (b.pts !== 0) continue;
      var ids = this.groupAt(b.x, b.y, b.w, b.h);
      if (ids) for (var k = 0; k < 4; k++) mark[ids[k]] = true;
    }
  };

  /* Split block i and resolve the consequences. Returns null (board untouched) if the
   * split is illegal, else a description of what happened, for drawing:
   *   exploded: blocks that exploded (as they were just before, with their last value)
   *   fell:     for blocks that fell, index after the move -> rows fallen
   *   refill:   indices (after the move) of blocks added at the top
   *   made:     indices (after the move) of blocks that became point blocks
   */
  Board.prototype.play = function (i) {
    if (!this.canSplit(i)) return null;
    var horiz = this.horizontalNext();
    var b = this.blocks[i], nb;
    if (horiz) {
      var hh = b.h / 2;
      nb = { x: b.x, y: b.y + hh, w: b.w, h: hh, pts: 0 };
      b.h = hh;
    } else {
      var hw = b.w / 2;
      nb = { x: b.x + hw, y: b.y, w: hw, h: b.h, pts: 0 };
      b.w = hw;
    }
    var newIdx = this.blocks.length;
    this.blocks.push(nb);
    paint(this.grid, nb, newIdx);
    this.moves += 1;
    var n = this.moves;
    var info = { horiz: horiz, exploded: [], fell: {}, refill: [], made: [], burnt: 0 };

    // New clusters: any 2x2 group of identical free blocks containing one of the halves.
    var mark = {};
    this.markGroupsContaining(i, mark);
    this.markGroupsContaining(newIdx, mark);
    var madeObjs = [];
    for (var j in mark) {
      var mb = this.blocks[j];
      mb.pts = n + 1; // counted down to n just below
      madeObjs.push(mb);
      info.burnt += mb.w * mb.h - 1;
    }

    // Count every point block down; the ones that run out explode.
    var keep = [], rowsDestroyed = 0;
    for (j = 0; j < this.blocks.length; j++) {
      var bj = this.blocks[j];
      if (bj.pts > 1) { bj.pts -= 1; keep.push(bj); }
      else if (bj.pts > 0) {
        info.exploded.push({ x: bj.x, y: bj.y, w: bj.w, h: bj.h, pts: bj.pts });
        rowsDestroyed |= ((1 << bj.h) - 1) << bj.y;
      } else keep.push(bj);
    }
    if (info.exploded.length) { this.blocks = keep; this.repaint(); }

    // Gravity: passes in list order until nothing moves.
    var fellObjs = [], dy = new Map(), falling = false, moved = true;
    while (moved) {
      moved = false;
      for (j = 0; j < this.blocks.length; j++) {
        bj = this.blocks[j];
        var bottom = bj.y + bj.h;
        if (bottom === H) continue;
        var dist = Infinity;
        for (var cx = bj.x; cx < bj.x + bj.w; cx++) {
          var d = 0;
          while (bottom + d < H && this.grid[(bottom + d) * W + cx] === EMPTY) d++;
          if (d < dist) dist = d;
          if (d === 0) break;
        }
        if (dist > 0) {
          paint(this.grid, bj, EMPTY);
          bj.y += dist;
          paint(this.grid, bj, j);
          dy.set(bj, (dy.get(bj) || 0) + dist);
          if (bj.pts > 0 && fellObjs.indexOf(bj) < 0) fellObjs.push(bj);
          falling = true;
          moved = true;
        }
      }
    }

    // Refill voids from the top.
    var before = this.blocks.length;
    var topVoid = false;
    for (cx = 0; cx < W; cx++) if (this.grid[cx] === EMPTY) { topVoid = true; break; }
    if (topVoid) this.refill(falling, rowsDestroyed);
    for (j = before; j < this.blocks.length; j++) info.refill.push(j);

    if (falling) {
      // After a fall, clusters can form anywhere; they are worth n.
      mark = {};
      this.markAllGroups(mark);
      for (j in mark) {
        mb = this.blocks[j];
        mb.pts = n;
        madeObjs.push(mb);
        info.burnt += mb.w * mb.h - 1;
      }
      // Point blocks that fell are halved (rounding down; 1 stays 1).
      for (j = 0; j < fellObjs.length; j++) if (fellObjs[j].pts > 1) fellObjs[j].pts = Math.floor(fellObjs[j].pts / 2);
    }

    for (j = 0; j < this.blocks.length; j++) {
      bj = this.blocks[j];
      if (dy.has(bj)) info.fell[j] = dy.get(bj);
      if (madeObjs.indexOf(bj) >= 0) info.made.push(j);
    }
    return info;
  };

  Board.prototype.refill = function (falling, rowsDestroyed) {
    var nv = [], c, r;
    for (c = 0; c < W; c++) {
      r = 0;
      if (!falling && rowsDestroyed !== 0) {
        // Voids that were there before are not filled unless something fell through them.
        while (r < H && this.grid[r * W + c] === EMPTY && (rowsDestroyed >> r & 1) === 1) r++;
      } else {
        while (r < H && this.grid[r * W + c] === EMPTY) r++;
      }
      nv.push(r);
    }
    for (;;) {
      var start = -1;
      for (c = 0; c < W; c++) if (nv[c] > 0) { start = c; break; }
      if (start < 0) break;
      // First pocket (run of columns with voids) from the left.
      var end = W - 1;
      for (c = start; c < W; c++) if (nv[c] === 0) { end = c - 1; break; }
      if (start === end) { nv[start] = 0; continue; } // one-column pockets are never filled
      var deepest = 0;
      for (c = start; c <= end; c++) if (nv[c] > deepest) deepest = nv[c];
      var vs = nv.indexOf(deepest);
      var vw = 0;
      while (vs + vw < W && nv[vs + vw] === deepest) vw++;
      var p2 = 1;
      while (p2 * 2 <= vw) p2 *= 2;
      vw = p2; // rounded down to a power of two
      if (vw === 1 || deepest % 2 === 1) {
        for (c = vs; c < vs + vw; c++) nv[c] -= 1;
        continue;
      }
      var hgt = 1;
      while (hgt * 2 <= deepest) hgt *= 2;
      var dist = Infinity;
      for (c = vs; c < vs + vw; c++) {
        var d = 0;
        while (hgt + d < H && this.grid[(hgt + d) * W + c] === EMPTY) d++;
        if (d < dist) dist = d;
      }
      var nb = { x: vs, y: dist, w: vw, h: hgt, pts: 0 };
      this.blocks.push(nb);
      paint(this.grid, nb, this.blocks.length - 1);
      for (c = vs; c < vs + vw; c++) nv[c] -= hgt;
    }
  };

  /* "x y w h pts;..." in block order: the format of `frames` (src/bin/frames.rs). */
  Board.prototype.toLine = function () {
    return this.blocks.map(function (b) { return b.x + ' ' + b.y + ' ' + b.w + ' ' + b.h + ' ' + b.pts; }).join(';');
  };

  function parseMoves(text) {
    return (text.match(/\d+/g) || []).map(Number);
  }

  return { W: W, H: H, EMPTY: EMPTY, Board: Board, parseMoves: parseMoves };
});
