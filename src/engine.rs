//! SPL-T game engine.
//!
//! A faithful port of the Python emulator `core.py` from github.com/josh-holder/SPLT
//! (itself derived from Craig Polley's flashingleds.net emulator). Sequences generated with
//! that emulator were replayed on a real iPhone to set the 1133-split world record, so its
//! behaviour - including its odd-looking refill rules - is treated as ground truth here.
//! `cargo run --release --bin replay <seq>` re-checks the published record sequences.
//!
//! The block list order is kept identical to the reference emulator (a split shrinks the
//! block in place and appends the new half; exploded blocks are removed preserving order;
//! refill blocks are appended). That lets the published record sequences, which are lists
//! of block indices, be replayed directly. See RULES.md for the rules in prose.

pub const W: usize = 8;
pub const H: usize = 16;
pub const CELLS: usize = W * H;
pub const MAXB: usize = CELLS;
pub const EMPTY: u8 = 255;
/// Build with `--features reference` to use straightforward whole-board versions of the
/// optimized steps (multi-pass gravity in list order, full grid repaint, full-board cluster
/// scan after falls). Results must be identical; `selfcheck` output is diffed between modes.
const REFERENCE: bool = cfg!(feature = "reference");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Block {
    pub x: u8,
    pub y: u8, // 0 is the TOP row; gravity pulls towards y = H-1
    pub w: u8,
    pub h: u8,
    /// 0 = ordinary (splittable) block; >0 = point block countdown.
    pub pts: u16,
}

impl Block {
    #[inline]
    pub fn area(&self) -> u32 {
        self.w as u32 * self.h as u32
    }
}

/// What happened during one move (for analysis / heuristics).
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveInfo {
    /// Number of blocks that became point blocks this move.
    pub new_point_blocks: u32,
    /// Total cell area of new point blocks.
    pub new_point_area: u32,
    /// Number of point blocks that exploded.
    pub exploded: u32,
    /// Cell area freed by explosions.
    pub exploded_area: u32,
    /// Number of point blocks that fell (and so were halved).
    pub halved: u32,
    /// Bitmask of columns containing an exploded block.
    pub exploded_cols: u8,
    /// Exploded cell area per column.
    pub exploded_col_area: [u8; W],
    /// Splitting capacity, sum(area - 1), of blocks that became point blocks.
    pub new_point_cap: u32,
    /// Splitting capacity of blocks added by the top refill.
    pub refill_cap: u32,
}

#[derive(Clone)]
pub struct Board {
    pub blocks: [Block; MAXB],
    /// Cell -> block index (EMPTY for a void). Row-major, row 0 at the top.
    pub grid: [u8; CELLS],
    pub len: u8,
    /// Number of splits made so far (= the score shown in the game).
    pub moves: u16,
    /// Smallest countdown among point blocks, 0 if there are none (maintained by `resolve`).
    next_expl: u16,
    /// Set when a refill added blocks without anything falling. The reference only scans the
    /// whole board for clusters after a fall, so an unclustered 2x2 group may then exist until
    /// the next fall. While this is clear, a fall can only create groups that contain a block
    /// that moved or was just added, so the post-fall scan can be restricted to those.
    dirty_groups: bool,
}

static START: std::sync::OnceLock<Board> = std::sync::OnceLock::new();

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        let mut b = Board { blocks: [Block::default(); MAXB], grid: [0; CELLS], len: 1, moves: 0, next_expl: 0, dirty_groups: false };
        b.blocks[0] = Block { x: 0, y: 0, w: W as u8, h: H as u8, pts: 0 };
        b
    }

    /// The position games start from: the empty board, unless `set_start` replaced it. Only
    /// what-if experiments replace it (`whatif`: the search machinery run from a constructed
    /// position, to measure what a different middle game would be worth).
    pub fn start() -> Board {
        START.get().cloned().unwrap_or_else(Board::new)
    }

    /// Make `b` the position that `start` returns, for the rest of the process.
    pub fn set_start(b: Board) {
        START.set(b).ok();
    }

    /// A board with exactly these blocks (in this order) after `moves` splits. The caller is
    /// responsible for the blocks tiling the board sensibly; `check_invariants` tells.
    pub fn from_parts(blocks: &[Block], moves: u16) -> Board {
        let mut b = Board { blocks: [Block::default(); MAXB], grid: [EMPTY; CELLS], len: blocks.len() as u8, moves, next_expl: 0, dirty_groups: false };
        b.blocks[..blocks.len()].copy_from_slice(blocks);
        b.repaint();
        b.next_expl = blocks.iter().filter(|x| x.pts > 0).map(|x| x.pts).min().unwrap_or(0);
        b
    }

    /// One line "moves" and one line "x y w h pts" per block; `from_text` reads it back.
    pub fn to_text(&self) -> String {
        let mut s = format!("{}\n", self.moves);
        for bl in self.blocks() {
            s.push_str(&format!("{} {} {} {} {}\n", bl.x, bl.y, bl.w, bl.h, bl.pts));
        }
        s
    }

    pub fn from_text(text: &str) -> Board {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let moves: u16 = lines.next().expect("board text: moves line").trim().parse().expect("board text: moves");
        let blocks: Vec<Block> = lines
            .map(|l| {
                let v: Vec<u16> = l.split_whitespace().map(|x| x.parse().expect("board text: number")).collect();
                Block { x: v[0] as u8, y: v[1] as u8, w: v[2] as u8, h: v[3] as u8, pts: v[4] }
            })
            .collect();
        Board::from_parts(&blocks, moves)
    }

    /// The first split is horizontal (cuts a block into top and bottom halves); then the
    /// direction strictly alternates.
    #[inline]
    pub fn horizontal_next(&self) -> bool {
        self.moves % 2 == 0
    }

    #[inline]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks[..self.len as usize]
    }

    #[inline]
    pub fn can_split(&self, i: usize) -> bool {
        if i >= self.len as usize {
            return false;
        }
        let b = &self.blocks[i];
        b.pts == 0 && if self.horizontal_next() { b.h > 1 } else { b.w > 1 }
    }

    pub fn legal_moves(&self, out: &mut Vec<u8>) {
        out.clear();
        let horiz = self.horizontal_next();
        for (i, b) in self.blocks().iter().enumerate() {
            if b.pts == 0 && if horiz { b.h > 1 } else { b.w > 1 } {
                out.push(i as u8);
            }
        }
    }

    pub fn num_legal_moves(&self) -> usize {
        let horiz = self.horizontal_next();
        self.blocks().iter().filter(|b| b.pts == 0 && if horiz { b.h > 1 } else { b.w > 1 }).count()
    }

    pub fn is_over(&self) -> bool {
        self.num_legal_moves() == 0
    }

    fn repaint(&mut self) {
        self.grid = [EMPTY; CELLS];
        for i in 0..self.len as usize {
            let b = self.blocks[i];
            paint(&mut self.grid, &b, i as u8);
        }
    }

    /// Returns indices of the four blocks forming a 2x2 group of identical free blocks whose
    /// top-left member has its top-left corner at (tlx, tly), if such a group exists.
    #[inline]
    fn group_at(&self, tlx: i32, tly: i32, w: u8, h: u8) -> Option<[u8; 4]> {
        let (wi, hi) = (w as i32, h as i32);
        if tlx < 0 || tly < 0 || tlx + 2 * wi > W as i32 || tly + 2 * hi > H as i32 {
            return None;
        }
        let mut out = [0u8; 4];
        for (k, (dx, dy)) in [(0, 0), (wi, 0), (0, hi), (wi, hi)].into_iter().enumerate() {
            let (cx, cy) = (tlx + dx, tly + dy);
            let id = self.grid[cy as usize * W + cx as usize];
            if id == EMPTY {
                return None;
            }
            let b = &self.blocks[id as usize];
            if b.x as i32 != cx || b.y as i32 != cy || b.w != w || b.h != h || b.pts != 0 {
                return None;
            }
            out[k] = id;
        }
        Some(out)
    }

    /// Mark every 2x2 group of identical free blocks that contains block `c`.
    fn mark_groups_containing(&self, c: usize, mark: &mut u128) {
        let b = self.blocks[c];
        let (w, h) = (b.w as i32, b.h as i32);
        for (rx, ry) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            if let Some(ids) = self.group_at(b.x as i32 - rx * w, b.y as i32 - ry * h, b.w, b.h) {
                for id in ids {
                    *mark |= 1u128 << id;
                }
            }
        }
    }

    /// Mark every 2x2 group of identical free blocks anywhere on the board.
    fn mark_all_groups(&self, mark: &mut u128) {
        for b in self.blocks() {
            if b.pts == 0 {
                if let Some(ids) = self.group_at(b.x as i32, b.y as i32, b.w, b.h) {
                    for id in ids {
                        *mark |= 1u128 << id;
                    }
                }
            }
        }
    }

    /// Would splitting block `i` now create a cluster (without considering later falls)?
    pub fn split_makes_cluster(&self, i: usize) -> bool {
        let mut t = self.clone();
        let horiz = t.horizontal_next();
        let b = t.blocks[i];
        let new_idx = t.len as usize;
        t.do_split(i, horiz);
        let _ = b;
        let mut mark = 0u128;
        t.mark_groups_containing(i, &mut mark);
        t.mark_groups_containing(new_idx, &mut mark);
        mark != 0
    }

    #[inline]
    fn do_split(&mut self, i: usize, horiz: bool) {
        let b = self.blocks[i];
        let new_idx = self.len as usize;
        let nb = if horiz {
            let hh = b.h / 2;
            self.blocks[i].h = hh;
            Block { x: b.x, y: b.y + hh, w: b.w, h: hh, pts: 0 }
        } else {
            let hw = b.w / 2;
            self.blocks[i].w = hw;
            Block { x: b.x + hw, y: b.y, w: hw, h: b.h, pts: 0 }
        };
        self.blocks[new_idx] = nb;
        self.len += 1;
        paint(&mut self.grid, &nb, new_idx as u8);
    }

    /// Split block `i` in the current direction and resolve the consequences.
    /// Returns None (and leaves the board untouched) if the split is illegal.
    pub fn play(&mut self, i: usize) -> Option<MoveInfo> {
        if !self.can_split(i) {
            return None;
        }
        let mut info = MoveInfo::default();
        let horiz = self.horizontal_next();
        let b = self.blocks[i];
        let new_idx = self.len as usize;

        // A vertical split of a block bridging a hole can leave one half unsupported.
        let bottom = (b.y + b.h) as usize;
        let maybe_fall = !horiz
            && bottom < H
            && (b.x as usize..(b.x + b.w) as usize).any(|x| self.grid[bottom * W + x] == EMPTY);
        let fall_cols = if maybe_fall { col_mask(&b) } else { 0 };

        // ---- 1. Split ------------------------------------------------------------------
        self.do_split(i, horiz);
        self.moves += 1;
        let n = self.moves;

        // ---- 2. New clusters: any 2x2 group of identical free blocks containing a half ----
        let mut mark: u128 = 0;
        self.mark_groups_containing(i, &mut mark);
        self.mark_groups_containing(new_idx, &mut mark);
        while mark != 0 {
            let j = mark.trailing_zeros() as usize;
            mark &= mark - 1;
            self.blocks[j].pts = n + 1; // counted down to n in step 3
            info.new_point_blocks += 1;
            info.new_point_area += self.blocks[j].area();
            info.new_point_cap += self.blocks[j].area() - 1;
        }

        self.resolve(n, 1, fall_cols, &mut info);
        Some(info)
    }

    /// Steps 3-8 of a move: count every point block down by `dec` (exploding the ones that
    /// run out), apply gravity, refill from the top, form clusters created by falling, and
    /// halve point blocks that fell. `n` is the split count used to value new clusters.
    /// `fall_cols`: columns where a split may have left a half unsupported.
    fn resolve(&mut self, n: u16, dec: u16, fall_cols: u8, info: &mut MoveInfo) {
        // ---- 3. Count down every point block ---------------------------------------------
        let t0 = phase::tick();
        let mut exploded: u128 = 0;
        let mut min_left = u16::MAX;
        for j in 0..self.len as usize {
            let p = self.blocks[j].pts;
            if p > dec {
                let q = p - dec;
                self.blocks[j].pts = q;
                min_left = min_left.min(q);
            } else if p > 0 {
                exploded |= 1u128 << j;
            }
        }
        phase::add(0, t0);

        // ---- 4. Remove exploded blocks -------------------------------------------------
        let t0 = phase::tick();
        let mut rows_destroyed: u32 = 0;
        let mut active = fall_cols; // columns where something may now be unsupported
        if exploded != 0 {
            let mut remap = [EMPTY; 256];
            let mut k = 0usize;
            for j in 0..self.len as usize {
                let bj = self.blocks[j];
                if exploded >> j & 1 == 1 {
                    info.exploded += 1;
                    info.exploded_area += bj.area();
                    let cm = col_mask(&bj);
                    info.exploded_cols |= cm;
                    active |= cm;
                    for c in bj.x..bj.x + bj.w {
                        info.exploded_col_area[c as usize] += bj.h;
                    }
                    rows_destroyed |= ((1u32 << bj.h) - 1) << bj.y;
                } else {
                    self.blocks[k] = bj;
                    remap[j] = k as u8;
                    k += 1;
                }
            }
            self.len = k as u8;
            if REFERENCE {
                self.repaint();
            } else {
                for c in self.grid.iter_mut() {
                    *c = remap[*c as usize];
                }
            }
        }
        phase::add(1, t0);

        // ---- 5. Gravity --------------------------------------------------------------------
        let t0 = phase::tick();
        let (mut fell, moved, falling) = if active != 0 {
            if REFERENCE { self.gravity_iterative() } else { self.gravity(active) }
        } else {
            (0, 0, false)
        };
        phase::add(2, t0);

        // ---- 6. Refill voids from the top (reference-emulator rules, quirks included) ----
        let t0 = phase::tick();
        let before_refill = self.len as usize;
        if self.grid[..W].contains(&EMPTY) {
            self.refill(falling, rows_destroyed);
            for j in before_refill..self.len as usize {
                info.refill_cap += self.blocks[j].area() - 1;
            }
        }
        phase::add(3, t0);

        // ---- 7. After a fall, look for clusters anywhere --------------------------------
        if falling {
            let t0 = phase::tick();
            let mut mark: u128 = 0;
            if REFERENCE || self.dirty_groups {
                self.mark_all_groups(&mut mark);
            } else {
                let mut cand = moved;
                for j in before_refill..self.len as usize {
                    cand |= 1u128 << j;
                }
                while cand != 0 {
                    let j = cand.trailing_zeros() as usize;
                    cand &= cand - 1;
                    if self.blocks[j].pts == 0 {
                        self.mark_groups_containing(j, &mut mark);
                    }
                }
            }
            self.dirty_groups = false;
            while mark != 0 {
                let j = mark.trailing_zeros() as usize;
                mark &= mark - 1;
                self.blocks[j].pts = n;
                min_left = min_left.min(n);
                info.new_point_blocks += 1;
                info.new_point_area += self.blocks[j].area();
                info.new_point_cap += self.blocks[j].area() - 1;
            }
            phase::add(4, t0);

            // ---- 8. Halve point blocks that fell (rounding down; 1 stays 1) -------------
            let t0 = phase::tick();
            while fell != 0 {
                let j = fell.trailing_zeros() as usize;
                fell &= fell - 1;
                let p = self.blocks[j].pts;
                if p > 1 {
                    self.blocks[j].pts = p / 2;
                    min_left = min_left.min(p / 2);
                }
                info.halved += 1;
            }
            phase::add(5, t0);
        } else if self.len as usize > before_refill {
            self.dirty_groups = true;
        }
        self.next_expl = if min_left == u16::MAX { 0 } else { min_left };
    }

    /// Let every unsupported block fall as far as it can. Returns (bitmask of point blocks
    /// that fell, bitmask of all blocks that moved, whether anything fell).
    ///
    /// The reference emulator loops "for each block in list order, fall if possible" until a
    /// pass moves nothing, which takes about one pass per level of stacking. The final state
    /// is unique: a block's resting place depends only on the final positions of the blocks
    /// beneath it, since blocks never pass each other. So handling blocks bottom-up reaches the
    /// same state in one pass. Also, the board was settled before this move, so a block can only
    /// lose its support in a column where something exploded, a split half may hang, or a
    /// block has just fallen. So only those `active` columns are scanned (bottom row first),
    /// and the set grows as blocks fall. `gravity_iterative` keeps the reference algorithm.
    fn gravity(&mut self, mut active: u8) -> (u128, u128, bool) {
        let mut done: u128 = 0;
        let mut fell: u128 = 0;
        let mut moved: u128 = 0;
        for row in (0..H - 1).rev() {
            let mut cols = active;
            while cols != 0 {
                let c = cols.trailing_zeros() as usize;
                cols &= cols - 1;
                let id = self.grid[row * W + c];
                if id == EMPTY || done >> id & 1 == 1 {
                    continue;
                }
                let j = id as usize;
                let bj = self.blocks[j];
                let bottom = (bj.y + bj.h) as usize;
                if bottom != row + 1 {
                    continue; // not this block's bottom row
                }
                done |= 1u128 << j;
                let mut dist = usize::MAX;
                for cx in bj.x as usize..(bj.x + bj.w) as usize {
                    let mut d = 0;
                    while bottom + d < H && self.grid[(bottom + d) * W + cx] == EMPTY {
                        d += 1;
                    }
                    dist = dist.min(d);
                    if d == 0 {
                        break;
                    }
                }
                if dist > 0 {
                    paint(&mut self.grid, &bj, EMPTY);
                    self.blocks[j].y += dist as u8;
                    let moved_b = self.blocks[j];
                    paint(&mut self.grid, &moved_b, j as u8);
                    moved |= 1u128 << j;
                    if bj.pts > 0 {
                        fell |= 1u128 << j;
                    }
                    active |= col_mask(&bj);
                }
            }
        }
        (fell, moved, moved != 0)
    }

    /// The reference emulator's gravity: repeat passes in list order until nothing moves.
    fn gravity_iterative(&mut self) -> (u128, u128, bool) {
        let mut fell: u128 = 0;
        let mut moved_mask: u128 = 0;
        let mut falling = false;
        loop {
            let mut moved = false;
            for j in 0..self.len as usize {
                let bj = self.blocks[j];
                let bottom = (bj.y + bj.h) as usize;
                if bottom == H {
                    continue;
                }
                let mut dist = usize::MAX;
                for cx in bj.x as usize..(bj.x + bj.w) as usize {
                    let mut d = 0;
                    while bottom + d < H && self.grid[(bottom + d) * W + cx] == EMPTY {
                        d += 1;
                    }
                    dist = dist.min(d);
                    if d == 0 {
                        break;
                    }
                }
                if dist > 0 {
                    paint(&mut self.grid, &bj, EMPTY);
                    self.blocks[j].y += dist as u8;
                    let moved_b = self.blocks[j];
                    paint(&mut self.grid, &moved_b, j as u8);
                    if bj.pts > 0 {
                        fell |= 1u128 << j;
                    }
                    moved_mask |= 1u128 << j;
                    falling = true;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        (fell, moved_mask, falling)
    }

    fn refill(&mut self, falling: bool, rows_destroyed: u32) {
        let mut num_voids = [0i32; W];
        for (c, nv) in num_voids.iter_mut().enumerate() {
            let mut r = 0;
            if !falling && rows_destroyed != 0 {
                // Pre-existing voids are not filled unless something fell through them.
                while r < H && self.grid[r * W + c] == EMPTY && rows_destroyed >> r & 1 == 1 {
                    r += 1;
                }
            } else {
                while r < H && self.grid[r * W + c] == EMPTY {
                    r += 1;
                }
            }
            *nv = r as i32;
        }
        while num_voids.iter().any(|&v| v != 0) {
            // First pocket (maximal run of columns with voids) from the left.
            let start = num_voids.iter().position(|&v| v > 0).unwrap();
            let mut end = W - 1;
            for (c, &v) in num_voids.iter().enumerate().skip(start) {
                if v == 0 {
                    end = c - 1;
                    break;
                }
            }
            if start == end {
                num_voids[start] = 0; // one-column pockets are never filled
                continue;
            }
            let deepest = *num_voids[start..=end].iter().max().unwrap();
            let vs = num_voids.iter().position(|&v| v == deepest).unwrap();
            let mut vw = num_voids[vs..].iter().take_while(|&&v| v == deepest).count();
            vw = 1 << (usize::BITS - 1 - vw.leading_zeros()); // round down to a power of 2
            if vw == 1 || deepest % 2 == 1 {
                for v in num_voids[vs..vs + vw].iter_mut() {
                    *v -= 1;
                }
                continue;
            }
            let hgt = 1usize << (31 - (deepest as u32).leading_zeros()); // largest pow2 <= deepest
            let mut dist = usize::MAX;
            for cx in vs..vs + vw {
                let mut d = 0;
                while hgt + d < H && self.grid[(hgt + d) * W + cx] == EMPTY {
                    d += 1;
                }
                dist = dist.min(d);
            }
            let nb = Block { x: vs as u8, y: dist as u8, w: vw as u8, h: hgt as u8, pts: 0 };
            let id = self.len as usize;
            self.blocks[id] = nb;
            self.len += 1;
            paint(&mut self.grid, &nb, id as u8);
            for v in num_voids[vs..vs + vw].iter_mut() {
                *v -= hgt as i32;
            }
        }
    }

    /// Smallest countdown among point blocks (= moves until the next explosion), if any.
    #[inline]
    pub fn next_explosion(&self) -> Option<u16> {
        if self.next_expl == 0 { None } else { Some(self.next_expl) }
    }

    /// Fast-forward as if `dt` = next_explosion() moves passed without any split creating
    /// new clusters: count down, explode, fall, refill, halve. Used for look-ahead estimates.
    /// Returns (dt, info) or None if there are no point blocks.
    pub fn passive_step(&mut self, elapsed: u16) -> Option<(u16, MoveInfo)> {
        let dt = self.next_explosion()?;
        let mut info = MoveInfo::default();
        let n = self.moves + elapsed + dt;
        self.resolve(n, dt, 0, &mut info);
        Some((dt, info))
    }

    /// Sanity checks: blocks in bounds, no overlaps, grid consistent, no floating block.
    pub fn check_invariants(&self) -> Option<String> {
        let mut g = [EMPTY; CELLS];
        for (i, b) in self.blocks().iter().enumerate() {
            if b.w == 0 || b.h == 0 || (b.x + b.w) as usize > W || (b.y + b.h) as usize > H {
                return Some(format!("block {i} out of bounds: {b:?}"));
            }
            for y in b.y..b.y + b.h {
                for x in b.x..b.x + b.w {
                    let c = &mut g[y as usize * W + x as usize];
                    if *c != EMPTY {
                        return Some(format!("block {i} overlaps block {}", *c));
                    }
                    *c = i as u8;
                }
            }
        }
        if g != self.grid {
            return Some("cached grid out of sync".into());
        }
        for (i, b) in self.blocks().iter().enumerate() {
            let bottom = (b.y + b.h) as usize;
            if bottom < H && (b.x..b.x + b.w).all(|x| g[bottom * W + x as usize] == EMPTY) {
                return Some(format!("block {i} is floating: {b:?}"));
            }
        }
        let min = self.blocks().iter().filter(|b| b.pts > 0).map(|b| b.pts).min().unwrap_or(0);
        if min != self.next_expl {
            return Some(format!("cached next explosion {} != actual {min}", self.next_expl));
        }
        if !self.dirty_groups {
            let mut mark = 0u128;
            self.mark_all_groups(&mut mark);
            if mark != 0 {
                return Some("unclustered 2x2 group present while dirty_groups is clear".into());
            }
        }
        None
    }

    /// Canonical, order-independent description of the position (for hashing / dedup).
    pub fn canonical(&self) -> [u32; CELLS] {
        let mut c = [0u32; CELLS];
        for b in self.blocks() {
            c[b.y as usize * W + b.x as usize] = 1 + (b.w as u32) | (b.h as u32) << 4 | (b.pts as u32) << 9;
        }
        c
    }

    pub fn hash64(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325 ^ self.moves as u64;
        for b in self.blocks() {
            let pos = b.y as u64 * W as u64 + b.x as u64;
            let v = pos | (b.w as u64) << 8 | (b.h as u64) << 16 | (b.pts as u64) << 24;
            // order-independent: combine per-block mixes with wrapping add
            let mut z = v.wrapping_add(0x9E3779B97F4A7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            h = h.wrapping_add(z ^ (z >> 31));
        }
        h
    }

    /// ASCII drawing; point blocks show their countdown, free blocks show their size.
    pub fn render(&self) -> String {
        let g = &self.grid;
        let mut s = String::new();
        s.push_str(&format!(
            "splits={} next={}\n",
            self.moves,
            if self.horizontal_next() { "horizontal (-)" } else { "vertical (|)" }
        ));
        for y in 0..H {
            for x in 0..W {
                let id = g[y * W + x];
                if id == EMPTY {
                    s.push_str("    ~");
                } else {
                    let b = &self.blocks[id as usize];
                    if b.pts > 0 {
                        s.push_str(&format!("{:>5}", b.pts));
                    } else {
                        s.push_str(&format!("{:>5}", format!("{}x{}", b.w, b.h)));
                    }
                }
            }
            s.push('\n');
        }
        s
    }
}

/// Optional phase timing (build with `--features prof`); compiled out otherwise.
pub mod phase {
    #[cfg(feature = "prof")]
    pub static TICKS: [std::sync::atomic::AtomicU64; 8] = [const { std::sync::atomic::AtomicU64::new(0) }; 8];
    pub const NAMES: [&str; 8] = ["countdown", "remove+repaint", "gravity", "refill", "fall-clusters", "halve", "split+clusters", "other"];
    #[inline(always)]
    pub fn tick() -> u64 {
        #[cfg(feature = "prof")]
        unsafe {
            return core::arch::x86_64::_rdtsc();
        }
        #[allow(unreachable_code)]
        0
    }
    #[inline(always)]
    pub fn add(_i: usize, _since: u64) {
        #[cfg(feature = "prof")]
        TICKS[_i].fetch_add(tick() - _since, std::sync::atomic::Ordering::Relaxed);
    }
}

#[inline]
fn col_mask(b: &Block) -> u8 {
    (((1u32 << b.w) - 1) << b.x) as u8
}

#[inline]
fn paint(g: &mut [u8; CELLS], b: &Block, v: u8) {
    for y in b.y as usize..(b.y + b.h) as usize {
        let row = y * W;
        g[row + b.x as usize..row + (b.x + b.w) as usize].fill(v);
    }
}

/// Parse a move list like "[0, 1, 1, 3]" or "(0, 1, 1, 3)" or whitespace separated ints.
pub fn parse_moves(text: &str) -> Vec<usize> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect()
}
