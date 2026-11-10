/* Animated SPL-T boards for the blog post: replays a solver game with splt-engine.js and
 * draws it on a canvas.
 *
 * Markup (the post's `splt` shortcode writes this):
 *
 *   <figure class="splt" data-games="3898" data-from="3640" data-to="3724" data-speed="3"
 *           data-mode="tween">
 *     <img class="splt-poster" src="/splt/poster-cycle.svg" alt="...">   (shown without JS)
 *     <figcaption>...</figcaption>
 *   </figure>
 *
 * data-games   one or more game ids from window.SPLT_GAMES, comma separated; several are
 *              drawn side by side at the same move number
 * data-titles  a title per board, separated by "|"
 * data-from, data-to   the range of moves played (the board after `from` moves is the
 *              first frame)
 * data-speed   moves per second ("lapse", "tween") or explosions per second ("events")
 * data-mode    lapse:  no in-between frames; exploding cells leave a short glow
 *              tween:  one move at a time; blocks slide when they fall
 *              events: jumps from explosion to explosion, with the falls animated, and
 *                      skips the cuts in between
 * data-cols    "a-b": draw only columns a to b (0-7), e.g. "6-7" for one pipe
 * data-labels  "0" hides the numbers on the point blocks
 * data-legend  "1" adds the colour key
 */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory(require('./splt-engine.js'), null);
  else root.SpltPlayer = factory(root.SpltEngine, root);
})(typeof self !== 'undefined' ? self : this, function (E, win) {
  'use strict';
  var W = E.W, H = E.H, EMPTY = E.EMPTY;

  // ---------------------------------------------------------------- colours
  // Point blocks are coloured by value on a log scale (one step per halving): pale yellow
  // for a block about to explode, deep purple for one just made late in the game.
  var RAMP = [[252, 253, 191], [254, 207, 146], [254, 159, 109], [247, 112, 92], [222, 73, 104], [183, 55, 121], [140, 41, 129], [100, 26, 128]];
  var BG = '#140003', FREE_FILL = 'rgba(216,203,204,0.07)', FREE_LINE = 'rgba(216,203,204,0.62)', INK = '#D8CBCC';

  function rampRgb(t) {
    t = Math.max(0, Math.min(1, t)) * (RAMP.length - 1);
    var i = Math.min(RAMP.length - 2, Math.floor(t)), f = t - i, a = RAMP[i], b = RAMP[i + 1];
    return [Math.round(a[0] + (b[0] - a[0]) * f), Math.round(a[1] + (b[1] - a[1]) * f), Math.round(a[2] + (b[2] - a[2]) * f)];
  }
  function valueT(v) { return Math.log(Math.max(1, v)) / Math.LN2 / 12; }
  function colorFor(v) { var c = rampRgb(valueT(v)); return 'rgb(' + c[0] + ',' + c[1] + ',' + c[2] + ')'; }
  function inkFor(v) { var c = rampRgb(valueT(v)); return (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) > 140 ? '#2a0508' : '#fff3ee'; }
  function fmt(n) { return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ','); }

  // ---------------------------------------------------------------- games
  var store = {};
  function decode(hex) { var out = []; for (var i = 0; i + 1 < hex.length; i += 2) out.push(parseInt(hex.substr(i, 2), 16)); return out; }
  function getGame(id, games) {
    if (store[id]) return store[id];
    var src = games[id];
    if (!src) throw new Error('SPL-T player: no game "' + id + '"');
    var moves = typeof src === 'string' ? decode(src) : src;
    var b = new E.Board(), snaps = [b.clone()], expl = [0];
    for (var k = 0; k < moves.length; k++) {
      var info = b.play(moves[k]);
      if (!info) throw new Error('SPL-T player: game "' + id + '" has an illegal move at ' + (k + 1));
      expl.push(info.exploded.length ? 1 : 0);
      if ((k + 1) % 64 === 0) snaps.push(b.clone());
    }
    return (store[id] = { id: id, moves: moves, snaps: snaps, expl: expl, n: moves.length });
  }
  function boardAt(g, m) {
    m = Math.max(0, Math.min(g.n, m));
    var s = m >> 6, b = g.snaps[s].clone();
    for (var k = s << 6; k < m; k++) b.play(g.moves[k]);
    return b;
  }

  // ---------------------------------------------------------------- geometry shared by canvas and SVG
  /* Groups of point blocks that touch and have the same value (a cluster), as rectangles in
   * cells when the group fills its bounding box: one number is drawn per group. */
  function labels(board) {
    var seen = new Uint8Array(W * H), out = [];
    for (var y0 = 0; y0 < H; y0++) for (var x0 = 0; x0 < W; x0++) {
      var id = board.grid[y0 * W + x0];
      if (seen[y0 * W + x0] || id === EMPTY || board.blocks[id].pts === 0) continue;
      var v = board.blocks[id].pts, stack = [[x0, y0]], cells = 0, minx = x0, maxx = x0, miny = y0, maxy = y0;
      seen[y0 * W + x0] = 1;
      while (stack.length) {
        var c = stack.pop(), x = c[0], y = c[1];
        cells++;
        if (x < minx) minx = x; if (x > maxx) maxx = x; if (y < miny) miny = y; if (y > maxy) maxy = y;
        var nb = [[x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]];
        for (var k = 0; k < 4; k++) {
          var nx = nb[k][0], ny = nb[k][1];
          if (nx < 0 || ny < 0 || nx >= W || ny >= H || seen[ny * W + nx]) continue;
          var nid = board.grid[ny * W + nx];
          if (nid === EMPTY || board.blocks[nid].pts !== v) continue;
          seen[ny * W + nx] = 1;
          stack.push([nx, ny]);
        }
      }
      if (cells === (maxx - minx + 1) * (maxy - miny + 1)) out.push({ x: minx, y: miny, w: maxx - minx + 1, h: maxy - miny + 1, v: v, id: id });
    }
    return out;
  }

  // ---------------------------------------------------------------- canvas drawing
  function ease(t) { return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2; }

  /* view: { cell, c0, c1, labels }. tr: { info, t } for a move in progress (t from 0 to 1),
   * or null. glow: per-cell brightness left by recent explosions, or null. */
  function draw(ctx, view, board, tr, glow) {
    var s = view.cell, c0 = view.c0, cols = view.c1 - view.c0 + 1;
    var e = tr ? ease(tr.t) : 1, info = tr ? tr.info : null;
    ctx.save();
    ctx.fillStyle = BG;
    ctx.fillRect(0, 0, cols * s, H * s);
    ctx.beginPath();
    ctx.rect(0, 0, cols * s, H * s);
    ctx.clip();
    var refill = {}, made = {};
    if (info) {
      info.refill.forEach(function (j) { refill[j] = 1; });
      info.made.forEach(function (j) { made[j] = 1; });
    }
    var g = s >= 18 ? 1 : 0.5; // half the gap between neighbouring blocks, in px
    for (var j = 0; j < board.blocks.length; j++) {
      var b = board.blocks[j];
      if (b.x + b.w - 1 < c0 || b.x > view.c1) continue;
      var off = info && info.fell[j] ? -info.fell[j] * (1 - e) : 0;
      var x = (b.x - c0) * s + g, y = (b.y + off) * s + g, w = b.w * s - 2 * g, h = b.h * s - 2 * g;
      ctx.globalAlpha = refill[j] ? e : 1;
      if (b.pts > 0) {
        ctx.fillStyle = colorFor(b.pts);
        ctx.fillRect(x, y, w, h);
        if (made[j] && e < 1) { ctx.fillStyle = 'rgba(255,255,255,' + (0.75 * (1 - e)).toFixed(3) + ')'; ctx.fillRect(x, y, w, h); }
      } else {
        ctx.fillStyle = FREE_FILL;
        ctx.fillRect(x, y, w, h);
        ctx.strokeStyle = FREE_LINE;
        ctx.lineWidth = 1;
        ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
      }
    }
    ctx.globalAlpha = 1;
    if (view.labels) {
      var ls = labels(board);
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (var k = 0; k < ls.length; k++) {
        var l = ls[k];
        if (l.x + l.w - 1 < c0 || l.x > view.c1) continue;
        // The part of the group inside the columns drawn.
        var lx0 = Math.max(l.x, c0), lw = Math.min(l.x + l.w - 1, view.c1) - lx0 + 1;
        var txt = String(l.v), boxw = lw * s - 6, boxh = l.h * s - 4;
        var px = Math.floor(Math.min(boxh * 0.62, boxw / (txt.length * 0.62), s * 0.8, 24));
        if (px < 7) continue;
        var loff = info && info.fell[l.id] ? -info.fell[l.id] * (1 - e) : 0;
        ctx.font = '600 ' + px + 'px "source-code-pro", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
        var tx = (lx0 - c0 + lw / 2) * s, ty = (l.y + loff + l.h / 2) * s;
        // A plate in the cluster's colour, so the lines between its blocks do not cut the digits.
        var tw = txt.length * px * 0.62 + 4;
        ctx.fillStyle = colorFor(l.v);
        ctx.fillRect(tx - tw / 2, ty - px * 0.56, tw, px * 1.12);
        ctx.fillStyle = inkFor(l.v);
        ctx.fillText(txt, tx, ty + px * 0.06);
      }
    }
    if (info && e < 1) {
      for (k = 0; k < info.exploded.length; k++) {
        var xb = info.exploded[k], grow = 1.5 * e;
        ctx.fillStyle = 'rgba(255,250,215,' + (0.95 * (1 - e)).toFixed(3) + ')';
        ctx.fillRect((xb.x - c0) * s - grow, xb.y * s - grow, xb.w * s + 2 * grow, xb.h * s + 2 * grow);
      }
    }
    if (glow) {
      for (var cy = 0; cy < H; cy++) for (var cx = c0; cx <= view.c1; cx++) {
        var a = glow[cy * W + cx];
        if (a > 0.03) { ctx.fillStyle = 'rgba(255,250,215,' + (0.85 * a).toFixed(3) + ')'; ctx.fillRect((cx - c0) * s, cy * s, s, s); }
      }
    }
    ctx.restore();
  }

  // ---------------------------------------------------------------- SVG (the posters shown without JavaScript)
  function renderSVG(boards, opts) {
    opts = opts || {};
    var s = opts.cell || 22, c0 = opts.c0 || 0, c1 = opts.c1 === undefined ? W - 1 : opts.c1, cols = c1 - c0 + 1;
    var gap = 28, titleH = opts.titles ? 22 : 0, bw = cols * s, width = boards.length * bw + (boards.length - 1) * gap, height = H * s + titleH;
    var out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ' + width + ' ' + height + '" width="' + width + '" height="' + height + '" font-family="Consolas, Menlo, monospace" font-weight="600">'];
    boards.forEach(function (board, bi) {
      var ox = bi * (bw + gap);
      if (opts.titles) out.push('<text x="' + (ox + bw / 2) + '" y="14" text-anchor="middle" font-size="13" font-weight="400" fill="' + INK + '">' + opts.titles[bi] + '</text>');
      out.push('<svg x="' + ox + '" y="' + titleH + '" width="' + bw + '" height="' + (H * s) + '" viewBox="0 0 ' + bw + ' ' + (H * s) + '">');
      out.push('<rect width="' + bw + '" height="' + (H * s) + '" fill="' + BG + '"/>');
      board.blocks.forEach(function (b) {
        if (b.x + b.w - 1 < c0 || b.x > c1) return;
        var x = (b.x - c0) * s + 1, y = b.y * s + 1, w = b.w * s - 2, h = b.h * s - 2;
        if (b.pts > 0) out.push('<rect x="' + x + '" y="' + y + '" width="' + w + '" height="' + h + '" fill="' + colorFor(b.pts) + '"/>');
        else out.push('<rect x="' + (x + 0.5) + '" y="' + (y + 0.5) + '" width="' + (w - 1) + '" height="' + (h - 1) + '" fill="' + FREE_FILL + '" stroke="' + FREE_LINE + '"/>');
      });
      if (opts.labels !== false) labels(board).forEach(function (l) {
        if (l.x + l.w - 1 < c0 || l.x > c1) return;
        var lx0 = Math.max(l.x, c0), lw = Math.min(l.x + l.w - 1, c1) - lx0 + 1;
        var txt = String(l.v), px = Math.floor(Math.min((l.h * s - 4) * 0.62, (lw * s - 6) / (txt.length * 0.62), s * 0.8, 24));
        if (px < 7) return;
        var tx = (lx0 - c0 + lw / 2) * s, ty = (l.y + l.h / 2) * s, tw = txt.length * px * 0.62 + 4;
        out.push('<rect x="' + (tx - tw / 2) + '" y="' + (ty - px * 0.56) + '" width="' + tw + '" height="' + (px * 1.12) + '" fill="' + colorFor(l.v) + '"/>');
        out.push('<text x="' + tx + '" y="' + (ty + px * 0.36) + '" text-anchor="middle" font-size="' + px + '" fill="' + inkFor(l.v) + '">' + txt + '</text>');
      });
      out.push('</svg>');
    });
    out.push('</svg>');
    return out.join('');
  }

  // ---------------------------------------------------------------- the player
  var CSS = [
    'figure.splt{margin:1.6em 0;}',
    'figure.splt figcaption{font-size:.85rem;color:#A59090;margin-top:.5em;}',
    '.splt-stage{display:flex;flex-wrap:wrap;gap:14px 28px;align-items:flex-start;justify-content:center;}',
    '.splt-board{display:flex;flex-direction:column;align-items:center;}',
    '.splt-title{font-size:.85rem;color:#D8CBCC;margin-bottom:4px;text-align:center;}',
    '.splt-board canvas{display:block;border:1px solid #5a3a3a;border-radius:2px;}',
    '.splt-pipes{display:flex;width:100%;font-size:.72rem;color:#A59090;line-height:1.5;}',
    '.splt-pipes span{flex:1 1 25%;text-align:center;}',
    '.splt-controls{display:flex;align-items:center;gap:10px;margin:10px auto 0;max-width:560px;font-size:.85rem;color:#D8CBCC;}',
    '.splt-controls button{background:#1E0000;color:#FFBBBB;border:1px solid #7a4a4a;border-radius:4px;min-width:2.4em;height:2em;padding:0 .5em;font:inherit;cursor:pointer;line-height:1;}',
    '.splt-controls button:hover{border-color:#FFBBBB;}',
    '.splt-controls input[type=range]{flex:1 1 auto;min-width:60px;accent-color:#FFBBBB;}',
    '.splt-readout{font-family:"source-code-pro",ui-monospace,Consolas,monospace;white-space:nowrap;min-width:9.5em;text-align:right;}',
    '.splt-legend{display:flex;align-items:center;gap:8px;justify-content:center;margin-top:8px;font-size:.78rem;color:#A59090;}',
    '.splt-legend i{display:inline-block;width:150px;height:10px;border-radius:2px;}',
    '.splt-legend b{display:inline-block;width:14px;height:10px;border:1px solid rgba(216,203,204,.62);background:rgba(216,203,204,.07);font-weight:400;}'
  ].join('');

  function rampCss() {
    var stops = [];
    for (var i = 0; i <= 10; i++) { var c = rampRgb(i / 10); stops.push('rgb(' + c[0] + ',' + c[1] + ',' + c[2] + ') ' + (i * 10) + '%'); }
    return 'linear-gradient(90deg,' + stops.join(',') + ')';
  }

  function Player(fig, games) {
    var d = fig.dataset, self = this;
    this.fig = fig;
    fig._splt = this; // handle for scripts and for debugging
    this.games = (d.games || '').split(',').map(function (id) { return getGame(id.trim(), games); });
    var longest = Math.max.apply(null, this.games.map(function (g) { return g.n; }));
    this.from = Math.max(0, parseInt(d.from || '0', 10));
    this.to = Math.min(longest, d.to ? parseInt(d.to, 10) : longest);
    this.speed = parseFloat(d.speed || '30');
    this.mode = d.mode || 'lapse';
    this.mult = 1;
    var cols = (d.cols || '0-7').split('-').map(Number);
    this.c0 = cols[0];
    this.c1 = cols[1];
    this.showLabels = d.labels !== '0';
    this.m = this.from;
    this.playing = false;
    this.userPaused = false;
    this.visible = false;
    this.hold = 0;      // seconds left to wait at the end before looping
    this.acc = 0;       // lapse: fractional moves owed; tween/events: seconds since the last step
    this.last = 0;
    var titles = d.titles ? d.titles.split('|') : [];

    var poster = fig.querySelector('.splt-poster');
    if (poster) poster.remove();
    var cap = fig.querySelector('figcaption');
    var stage = document.createElement('div');
    stage.className = 'splt-stage';
    this.views = this.games.map(function (g, i) {
      var wrap = document.createElement('div');
      wrap.className = 'splt-board';
      if (titles[i]) { var t = document.createElement('div'); t.className = 'splt-title'; t.textContent = titles[i]; wrap.appendChild(t); }
      var cv = document.createElement('canvas');
      cv.setAttribute('role', 'img');
      cv.setAttribute('aria-label', 'SPL-T board, animated' + (titles[i] ? ': ' + titles[i] : ''));
      wrap.appendChild(cv);
      if (self.c0 === 0 && self.c1 === W - 1) {
        // Pipe letters under a full board (the post calls the pipes A to D, left to right).
        var row = document.createElement('div');
        row.className = 'splt-pipes';
        row.setAttribute('aria-hidden', 'true');
        'ABCD'.split('').forEach(function (ch) { var sp = document.createElement('span'); sp.textContent = ch; row.appendChild(sp); });
        wrap.appendChild(row);
      }
      stage.appendChild(wrap);
      return { game: g, canvas: cv, ctx: cv.getContext('2d'), board: boardAt(g, self.m), tr: null, glow: new Float32Array(W * H) };
    });
    fig.insertBefore(stage, cap);

    var ctl = document.createElement('div');
    ctl.className = 'splt-controls';
    this.btn = document.createElement('button');
    this.btn.type = 'button';
    this.seek = document.createElement('input');
    this.seek.type = 'range';
    this.seek.min = this.from;
    this.seek.max = this.to;
    this.seek.step = 1;
    this.seek.value = this.m;
    this.seek.setAttribute('aria-label', 'Move');
    this.readout = document.createElement('span');
    this.readout.className = 'splt-readout';
    this.spd = document.createElement('button');
    this.spd.type = 'button';
    this.spd.setAttribute('aria-label', 'Playback speed');
    ctl.appendChild(this.btn);
    ctl.appendChild(this.seek);
    ctl.appendChild(this.readout);
    ctl.appendChild(this.spd);
    fig.insertBefore(ctl, cap);
    if (d.legend === '1') {
      var lg = document.createElement('div');
      lg.className = 'splt-legend';
      lg.innerHTML = '<b></b><span>free block</span><span style="margin-left:10px">point block worth 1</span><i style="background:' + rampCss() + '"></i><span>4,000</span>';
      fig.insertBefore(lg, cap);
    }

    this.btn.addEventListener('click', function () {
      if (self.playing) { self.userPaused = true; self.pause(); }
      else { self.userPaused = false; if (self.m >= self.to) self.jump(self.from); self.play(); }
    });
    this.spd.addEventListener('click', function () {
      var order = [1, 2, 4, 0.25, 0.5];
      self.mult = order[(order.indexOf(self.mult) + 1) % order.length];
      self.ui();
    });
    this.seek.addEventListener('input', function () {
      self.userPaused = true;
      self.pause();
      self.jump(parseInt(self.seek.value, 10));
    });
    this.layout();
    this.ui();
    this.render();
    if (typeof ResizeObserver !== 'undefined') new ResizeObserver(function () { self.layout(); self.render(); }).observe(fig);
    var still = win.matchMedia && win.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (still) this.userPaused = true;
    if (typeof IntersectionObserver !== 'undefined') {
      new IntersectionObserver(function (es) {
        self.visible = es[0].isIntersecting;
        if (self.visible && !self.userPaused) self.play(); else self.pause();
      }, { threshold: 0.35 }).observe(stage);
    } else if (!still) this.play();
  }

  Player.prototype.layout = function () {
    var cols = this.c1 - this.c0 + 1, n = this.views.length;
    var avail = Math.max(120, this.fig.clientWidth || 600) - (n - 1) * 28 - 2 * n;
    var maxCell = cols <= 2 ? 44 : (n > 1 ? 24 : 26);
    var cell = Math.max(10, Math.min(maxCell, Math.floor(avail / (cols * n))));
    var dpr = Math.min(2.5, win.devicePixelRatio || 1);
    this.cell = cell;
    for (var i = 0; i < n; i++) {
      var v = this.views[i], cw = cols * cell, ch = H * cell;
      v.canvas.style.width = cw + 'px';
      v.canvas.style.height = ch + 'px';
      v.canvas.width = Math.round(cw * dpr);
      v.canvas.height = Math.round(ch * dpr);
      v.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
  };

  Player.prototype.ui = function () {
    this.btn.textContent = this.playing ? '❚❚' : '▶';
    this.btn.setAttribute('aria-label', this.playing ? 'Pause' : 'Play');
    this.spd.textContent = (this.mult < 1 ? String(this.mult).replace('0.', '.') : this.mult) + '×';
    this.seek.value = this.m;
    var over = this.views.length === 1 && this.m >= this.views[0].game.n;
    this.readout.textContent = 'move ' + fmt(this.m) + (over ? ' (game over)' : '');
  };

  Player.prototype.render = function () {
    for (var i = 0; i < this.views.length; i++) {
      var v = this.views[i];
      draw(v.ctx, { cell: this.cell, c0: this.c0, c1: this.c1, labels: this.showLabels && this.cell >= 13 }, v.board, v.tr, this.mode === 'lapse' ? v.glow : null);
    }
  };

  Player.prototype.jump = function (m) {
    this.m = Math.max(this.from, Math.min(this.to, m));
    for (var i = 0; i < this.views.length; i++) {
      var v = this.views[i];
      v.board = boardAt(v.game, this.m);
      v.tr = null;
      v.glow.fill(0);
    }
    this.acc = 0;
    this.hold = 0;
    this.ui();
    this.render();
  };

  /* Advance every board by one move. Returns true if any of them had an explosion. */
  Player.prototype.step = function (animate) {
    var boom = false;
    for (var i = 0; i < this.views.length; i++) {
      var v = this.views[i];
      if (this.m >= v.game.n) { v.tr = null; continue; }
      var info = v.board.play(v.game.moves[this.m]);
      if (info.exploded.length) {
        boom = true;
        for (var k = 0; k < info.exploded.length; k++) {
          var x = info.exploded[k];
          for (var cy = x.y; cy < x.y + x.h; cy++) for (var cx = x.x; cx < x.x + x.w; cx++) v.glow[cy * W + cx] = 1;
        }
      }
      v.tr = animate ? { info: info, t: 0 } : null;
    }
    this.m++;
    return boom;
  };

  Player.prototype.play = function () {
    if (this.playing) return;
    this.playing = true;
    this.last = 0;
    this.ui();
    var self = this;
    win.requestAnimationFrame(function f(ts) {
      if (!self.playing) return;
      var dt = self.last ? Math.min(0.1, (ts - self.last) / 1000) : 0;
      self.last = ts;
      self.tick(dt);
      win.requestAnimationFrame(f);
    });
  };

  Player.prototype.pause = function () {
    if (!this.playing) return;
    this.playing = false;
    this.ui();
  };

  Player.prototype.tick = function (dt) {
    var rate = this.speed * this.mult, i;
    if (this.m >= this.to) {
      // Finish the last move's animation, wait, then start again.
      var busy = false;
      for (i = 0; i < this.views.length; i++) {
        var v = this.views[i];
        if (v.tr) { v.tr.t += dt / 0.4; if (v.tr.t >= 1) v.tr = null; else busy = true; }
        for (var c = 0; c < W * H; c++) v.glow[c] *= Math.exp(-dt / 0.18);
      }
      this.render();
      if (!busy) {
        this.hold += dt;
        if (this.hold > 1.8) this.jump(this.from);
      }
      return;
    }
    if (this.mode === 'lapse') {
      this.acc += dt * rate;
      var decay = Math.exp(-dt / 0.18);
      for (i = 0; i < this.views.length; i++) for (c = 0; c < W * H; c++) this.views[i].glow[c] *= decay;
      while (this.acc >= 1 && this.m < this.to) { this.step(false); this.acc -= 1; }
    } else {
      var period = 1 / rate, dur = Math.min(period * 0.9, 0.45);
      this.acc += dt;
      for (i = 0; i < this.views.length; i++) {
        v = this.views[i];
        if (v.tr) { v.tr.t += dt / dur; if (v.tr.t >= 1) v.tr = null; }
      }
      if (this.acc >= period) {
        this.acc -= period;
        if (this.acc > period) this.acc = 0;
        if (this.mode === 'events') {
          // Play silently up to the move before the next explosion, then animate that move.
          var guard = 0;
          while (this.m < this.to - 1 && !this.nextExplodes() && guard++ < 5000) this.step(false);
          this.step(true);
        } else this.step(true);
      }
    }
    this.ui();
    this.render();
  };

  Player.prototype.nextExplodes = function () {
    for (var i = 0; i < this.views.length; i++) { var g = this.views[i].game; if (this.m < g.n && g.expl[this.m + 1]) return true; }
    return false;
  };

  function mountAll() {
    if (!win || !win.document) return;
    var figs = win.document.querySelectorAll('figure.splt:not([data-splt-ready])');
    if (!figs.length) return;
    if (!win.document.getElementById('splt-css')) {
      var st = win.document.createElement('style');
      st.id = 'splt-css';
      st.textContent = CSS;
      win.document.head.appendChild(st);
    }
    for (var i = 0; i < figs.length; i++) {
      figs[i].setAttribute('data-splt-ready', '1');
      try { new Player(figs[i], win.SPLT_GAMES || {}); } catch (err) { if (win.console) win.console.error(err); }
    }
  }
  if (win && win.document) {
    if (win.document.readyState === 'loading') win.document.addEventListener('DOMContentLoaded', mountAll);
    else mountAll();
  }

  return { mountAll: mountAll, renderSVG: renderSVG, getGame: getGame, boardAt: boardAt, colorFor: colorFor, labels: labels };
});
