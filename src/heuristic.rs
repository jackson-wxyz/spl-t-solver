//! Faithful port of Josh Holder's move-scoring heuristic (`weighting.py` in
//! github.com/josh-holder/SPLT, the version that produced the 1053-1208 record games).
//!
//! `weights(board)` returns one weight per block, in block-index order, exactly as the
//! Python `findWeights` does (0 for blocks that cannot be split now). Quirks of the
//! original are preserved on purpose, since the whole point is to reproduce the policy
//! that generated the record sequences; `cargo run --release --bin josh_dump` prints the
//! weights along a sequence in the same format as the Python reference so the two can be
//! diffed. Comments quote the Python variable names.

use crate::engine::{Board, H, MAXB, W};

/// Point values of the point blocks "below" a block, bottom-most first (Python
/// `pointBlocksBelow[i]`). Chains never exceed the board height.
#[derive(Clone, Copy, Default)]
pub struct Chain {
    pub vals: [u16; 16],
    pub len: u8,
}

impl Chain {
    #[inline]
    fn push(&mut self, v: u16) {
        if (self.len as usize) < self.vals.len() {
            self.vals[self.len as usize] = v;
            self.len += 1;
        }
    }
    #[inline]
    fn contains(&self, v: u16) -> bool {
        self.vals[..self.len as usize].contains(&v)
    }
    #[inline]
    pub fn last(&self) -> Option<u16> {
        if self.len == 0 { None } else { Some(self.vals[self.len as usize - 1]) }
    }
    #[inline]
    pub fn as_slice(&self) -> &[u16] {
        &self.vals[..self.len as usize]
    }
}

/// Everything `findWeights` needs, exposed so other evaluators can reuse it.
pub struct Analysis {
    /// Python `splitsUntilFall[i]`: moves until block i falls (0 = it will not fall
    /// before the point blocks beneath it run out, or it sits on the floor).
    pub suf: [u16; MAXB],
    pub below: [Chain; MAXB],
    /// Bitmask of the blocks that would form clusters if block i were split now
    /// (bit `len` stands for the new half). 0 if no cluster.
    pub cluster: [u128; MAXB],
    /// Python `lowest_in_col`: per 2-column pipe, the lowest 2-row slot holding a free
    /// block's top-left corner.
    pub lowest_in_col: [i32; 4],
    pub hor_splits: i32,
    pub ver_splits: i32,
}

/// Python `findCluster` for one block: the 2x2 groups of identical free blocks that would
/// exist after splitting block `i` in the current direction and contain one of its halves.
pub fn cluster_mask(b: &Board, i: usize) -> u128 {
    let bl = b.blocks[i];
    let horiz = b.horizontal_next();
    let (x, y, w, h) = (bl.x as i32, bl.y as i32, bl.w as i32, bl.h as i32);
    let (gw, gh) = if horiz { (w, h / 2) } else { (w / 2, h) };
    let newi = b.len as usize;
    let mut mask = 0u128;
    // Top-left corners of the candidate groups.
    let (dxs, dys): (&[i32], &[i32]) = if horiz { (&[-gw, 0], &[-gh, 0, gh]) } else { (&[-gw, 0, gw], &[-gh, 0]) };
    for &dx in dxs {
        for &dy in dys {
            let (tx, ty) = (x + dx, y + dy);
            if tx < 0 || ty < 0 || tx + 2 * gw > W as i32 || ty + 2 * gh > H as i32 {
                continue;
            }
            let mut m = 0u128;
            let mut ok = true;
            for (mx, my) in [(tx, ty), (tx + gw, ty), (tx, ty + gh), (tx + gw, ty + gh)] {
                if mx == x && my == y {
                    m |= 1u128 << i; // the shrunken original (top / left half)
                } else if mx == x + (if horiz { 0 } else { gw }) && my == y + (if horiz { gh } else { 0 }) {
                    m |= 1u128 << newi; // the new half
                } else {
                    let id = b.grid[my as usize * W + mx as usize];
                    if id == crate::engine::EMPTY {
                        ok = false;
                        break;
                    }
                    let o = &b.blocks[id as usize];
                    if o.pts != 0 || o.x as i32 != mx || o.y as i32 != my || o.w as i32 != gw || o.h as i32 != gh {
                        ok = false;
                        break;
                    }
                    m |= 1u128 << id;
                }
            }
            if ok {
                mask |= m;
            }
        }
    }
    mask
}

pub fn analyse(b: &Board) -> Analysis {
    let n = b.len as usize;
    let blocks = b.blocks();
    // boxesInRow[r] for r = 1..=H: blocks with y < r <= y + h, in index order.
    let mut rows = [[0u8; MAXB]; H + 1];
    let mut rown = [0usize; H + 1];
    let (mut hs, mut vs) = (0i32, 0i32);
    for (i, bl) in blocks.iter().enumerate() {
        if bl.pts == 0 {
            hs += bl.h as i32 - 1;
            vs += bl.w as i32 - 1;
        }
        for r in bl.y as usize + 1..=(bl.y + bl.h) as usize {
            rows[r][rown[r]] = i as u8;
            rown[r] += 1;
        }
    }
    let mut suf = [0u16; MAXB];
    let mut below = [Chain::default(); MAXB];
    // Bottom row.
    for &i in &rows[H][..rown[H]] {
        let i = i as usize;
        suf[i] = blocks[i].pts;
        below[i] = Chain::default();
        below[i].push(blocks[i].pts);
    }
    for r in (1..H).rev() {
        for &i in &rows[r][..rown[r]] {
            let i = i as usize;
            let bl = blocks[i];
            let mut max_fall: u16 = 0;
            for &o in &rows[r + 1][..rown[r + 1]] {
                let o = o as usize;
                let ob = blocks[o];
                if ob.x + ob.w > bl.x && ob.x < bl.x + bl.w {
                    if suf[o] == 0 {
                        max_fall = 0;
                        break;
                    }
                    if suf[o] > max_fall {
                        max_fall = suf[o];
                        below[i] = below[o];
                    }
                }
            }
            if bl.pts != 0 && max_fall != 0 {
                if bl.pts > max_fall && !below[i].contains(bl.pts) {
                    below[i].push(bl.pts);
                }
                suf[i] = bl.pts.min(max_fall);
            } else {
                suf[i] = max_fall;
            }
        }
    }
    let mut lowest_in_col = [0i32; 4];
    for bl in blocks {
        if bl.pts == 0 {
            let (row, col) = (bl.y as i32 / 2, bl.x as usize / 2);
            if row > lowest_in_col[col] {
                lowest_in_col[col] = row;
            }
        }
    }
    let mut cluster = [0u128; MAXB];
    for i in 0..n {
        if b.can_split(i) {
            cluster[i] = cluster_mask(b, i);
        }
    }
    Analysis { suf, below, cluster, lowest_in_col, hor_splits: hs, ver_splits: vs }
}

/// Python `findWeights`: one weight per block (0 for unsplittable blocks).
pub fn weights(b: &Board) -> Vec<f64> {
    let a = analyse(b);
    weights_from(b, &a)
}

pub fn weights_from(b: &Board, a: &Analysis) -> Vec<f64> {
    let n = b.len as usize;
    let blocks = b.blocks();
    let horiz = b.horizontal_next();
    let total_splits = (a.hor_splits + a.ver_splits) as f64;
    let imb = (a.hor_splits - a.ver_splits) as f64;
    let curr = b.moves as f64;
    let curr_i = b.moves as i64;
    let min_suf = a.suf[..n].iter().copied().min().unwrap_or(0) as f64;
    let about_to_lose = total_splits < min_suf;
    let board_h = H as f64;

    let mut out = vec![0.0f64; n];
    for i in 0..n {
        let bl = blocks[i];
        if !(bl.pts == 0 && b.can_split(i)) {
            continue;
        }
        let (w, h) = (bl.w as f64, bl.h as f64);
        let mask = a.cluster[i];
        let create = mask != 0;
        let mut weight = 0.0f64;

        // ---- imbalance ------------------------------------------------------------
        if horiz {
            if !create {
                let vadd = w - 1.0;
                let mut wta;
                if imb <= 0.0 {
                    wta = (-15.0 / 7.0) * vadd;
                    wta *= 4.0 * imb.abs() / total_splits;
                } else {
                    let pct = vadd / imb.abs();
                    wta = -12.5 * pct * pct + 27.5 * pct;
                    wta *= 4.0 * imb.abs() / total_splits;
                }
                if about_to_lose {
                    wta *= 100.0;
                }
                weight += wta;
            }
        } else if !create {
            let hadd = h - 1.0;
            let mut wta;
            if imb >= 0.0 {
                wta = (-15.0 / 7.0) * hadd;
                wta *= 4.0 * imb.abs() / total_splits;
            } else {
                let pct = hadd / imb.abs();
                wta = -12.5 * pct * pct + 27.5 * pct;
                wta *= 4.0 * imb.abs() / total_splits;
            }
            weight += wta;
        }

        // ---- aspect ratio -------------------------------------------------------------
        let ar = w.max(h) / w.min(h);
        let nar = if horiz { w.max(h / 2.0) / w.min(h / 2.0) } else { (w / 2.0).max(h) / (w / 2.0).min(h) };
        let mut wta = 0.0;
        if nar - ar > 1.0 {
            if nar - ar >= 4.0 {
                wta = -20.0;
            } else if nar - ar >= 2.0 {
                wta = -10.0;
                if curr_i > 450 {
                    wta = -15.0;
                }
            }
        }
        weight += wta;

        // ---- point block weight ---------------------------------------------------------
        let mut wta = 0.0;
        let suf = a.suf[i] as i64;
        if suf != 0 {
            let chain = &a.below[i];
            let extra = chain.len as i64 - 1;
            let difference = (curr_i + 1) - (chain.last().unwrap_or(0) as i64 - 1);
            let pow = |e: i64| if e >= 0 { 1i64 << e } else { 0 };
            if create {
                if difference >= pow(extra) {
                    let turns = (suf as f64 / 2.0).ceil();
                    wta = 29.53 / (1.0 + 0.1518 * (1.14 * turns).exp()) + 20.0;
                    if difference < pow(extra + 1) {
                        wta *= 0.85;
                    }
                }
            } else {
                let (ca, cb, cc) = (13.0, 1.6, -0.2);
                if difference >= pow(extra) || suf <= 5 {
                    let s = suf as f64;
                    wta = if 2 < suf && suf < 13 { cc * s * s + cb * s + ca } else { 0.0 };
                    if suf <= 2 && chain.len > 1 {
                        let min_rest = chain.as_slice()[1..].iter().copied().min().unwrap() as i64;
                        let till_fall = ((min_rest - suf) as f64 / 2.0).ceil() as i64;
                        // (sic) the original evaluates the parabola at splitsUntilFall, not tillFall
                        wta = if 2 < till_fall && till_fall < 13 { cc * s * s + cb * s + ca } else { 0.0 };
                    }
                }
            }
            if curr_i > 300 {
                let (box_row, box_col) = (bl.y as i32 / 2, bl.x as usize / 2);
                if box_row < a.lowest_in_col[box_col] {
                    wta *= 0.5;
                }
            }
        }
        weight += wta;

        // ---- next highest in column -------------------------------------------------
        if a.below[i].len != 0 && curr_i > 800 {
            let pct = (curr - a.below[i].last().unwrap() as f64) / curr;
            weight += 9.0 * pct;
        }

        // ---- height -------------------------------------------------------------------
        let wta = if create {
            let mut max_height = 0.0f64;
            let mut m = mask;
            while m != 0 {
                let ci = m.trailing_zeros() as usize;
                m &= m - 1;
                let cand = if ci < n { (blocks[ci].y + blocks[ci].h) as f64 } else { 0.0 };
                if cand > max_height {
                    max_height = cand;
                }
            }
            20.0 * (max_height / board_h).powi(2)
        } else {
            10.0 * ((bl.y + bl.h) as f64 / board_h).powi(2)
        };
        weight += wta;

        // ---- point block size -----------------------------------------------------------
        if create {
            let cluster_squares = (w * h) / 2.0 * mask.count_ones() as f64;
            if cluster_squares > 4.0 {
                weight += -60.0 * (cluster_squares / total_splits) * (curr / 200.0);
            }
        }

        // ---- cluster between rows / columns ---------------------------------------------
        let mut wta = 0.0;
        if create {
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (8i32, 16i32, 0i32, 0i32);
            let mut m = mask;
            while m != 0 {
                let ci = m.trailing_zeros() as usize;
                m &= m - 1;
                if ci < n {
                    let c = blocks[ci];
                    max_x = max_x.max(c.x as i32);
                    max_y = max_y.max(c.y as i32);
                    min_x = min_x.min(c.x as i32);
                    min_y = min_y.min(c.y as i32);
                }
            }
            let (top, left) = (min_y, min_x);
            let (bottom, right) = if horiz {
                (max_y + bl.h as i32 / 2, max_x + bl.w as i32)
            } else {
                (max_y + bl.h as i32, max_x + bl.w as i32 / 2)
            };
            if bottom % 2 != 0 || top % 2 != 0 {
                wta -= 50.0;
            }
            if right % 2 != 0 || left % 2 != 0 {
                wta -= 50.0;
            }
        }
        weight += wta;

        // ---- wide blocks ----------------------------------------------------------------
        if horiz {
            if bl.h > 2 && bl.w > 2 {
                weight += -5.0;
            }
        } else if bl.w > 2 {
            weight += 5.0;
        }

        // ---- forced triple edge case ------------------------------------------------
        if bl.w == 2 && bl.h == 1 && !horiz {
            let (mut upright, mut small) = (false, false);
            for o in blocks {
                if (o.x == bl.x || o.x == bl.x + 1) && o.w == 1 && o.y == bl.y + 1 {
                    if o.h == 2 {
                        upright = true;
                    } else if o.h == 1 {
                        small = true;
                    }
                }
            }
            if upright && small {
                weight += -1.0;
            }
        }

        out[i] = weight;
    }

    // Shift so the worst block has (almost) no weight.
    let mut min_w = out.iter().copied().fold(f64::INFINITY, f64::min);
    if min_w <= 0.0 {
        min_w = -min_w;
        for x in out.iter_mut() {
            *x += min_w + 0.01;
        }
    }
    for i in 0..n {
        if !b.can_split(i) {
            out[i] = 0.0;
        }
    }
    out
}

/// The greedy choice of the reference scripts: the highest weight, lowest index on ties
/// (Python `max(range(n), key=weights.__getitem__)`). None if no move is possible.
pub fn best_move(ws: &[f64]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &w) in ws.iter().enumerate() {
        if w > 0.0 && best.is_none_or(|(_, bw)| w > bw) {
            best = Some((i, w));
        }
    }
    best.map(|(i, _)| i)
}

/// Play greedily with the heuristic until the game ends. Returns the number of splits.
pub fn greedy_play(b: &mut Board, max_moves: usize) -> usize {
    let start = b.moves as usize;
    while (b.moves as usize) < start + max_moves {
        let ws = weights(b);
        let Some(m) = best_move(&ws) else { break };
        b.play(m).unwrap();
    }
    b.moves as usize - start
}

/// Rollout / play policies built on the heuristic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// The reference scripts' choice: the highest weight.
    Greedy,
    /// Highest weight among moves that do not create point blocks bigger than 1x1; falls
    /// back to the greedy choice when every move does. (ParanoidPlay's backtracking
    /// explores exactly these alternatives.)
    AvoidBig,
}

impl std::str::FromStr for Policy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "greedy" => Ok(Policy::Greedy),
            "avoidbig" => Ok(Policy::AvoidBig),
            _ => Err(format!("unknown policy {s}")),
        }
    }
}

/// Would splitting block `i` (which has cluster mask `mask`) create point blocks bigger
/// than 1x1?
#[inline]
pub fn makes_big_cluster(b: &Board, i: usize, mask: u128) -> bool {
    if mask == 0 {
        return false;
    }
    let bl = b.blocks[i];
    !(if b.horizontal_next() { bl.w == 1 && bl.h == 2 } else { bl.w == 2 && bl.h == 1 })
}

pub fn choose(b: &Board, pol: Policy) -> Option<usize> {
    let a = analyse(b);
    let ws = weights_from(b, &a);
    match pol {
        Policy::Greedy => best_move(&ws),
        Policy::AvoidBig => {
            let mut best: Option<(usize, f64)> = None;
            let mut fallback: Option<(usize, f64)> = None;
            for (i, &w) in ws.iter().enumerate() {
                if w <= 0.0 {
                    continue;
                }
                let slot = if makes_big_cluster(b, i, a.cluster[i]) { &mut fallback } else { &mut best };
                if slot.is_none_or(|(_, bw)| w > bw) {
                    *slot = Some((i, w));
                }
            }
            best.or(fallback).map(|(i, _)| i)
        }
    }
}

/// Play `pol` until the game ends or `cap` moves (0 = no cap). Returns (moves played, over).
pub fn play_out(b: &mut Board, pol: Policy, cap: usize) -> (usize, bool) {
    let mut n = 0usize;
    loop {
        if cap > 0 && n >= cap {
            return (n, false);
        }
        let Some(m) = choose(b, pol) else { return (n, true) };
        b.play(m).unwrap();
        n += 1;
    }
}

#[inline]
fn xorshift(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

/// Sample a move with probability proportional to exp(weight / temp) (temp <= 0: proportional
/// to the weight itself, as in Josh Holder's weighted_random.py).
pub fn choose_stochastic(b: &Board, temp: f64, rng: &mut u64) -> Option<usize> {
    let ws = weights(b);
    let mut probs = [0f64; MAXB];
    let mut total = 0f64;
    let wmax = ws.iter().copied().fold(0.0, f64::max);
    if wmax <= 0.0 {
        return None;
    }
    for (i, &w) in ws.iter().enumerate() {
        if w > 0.0 {
            let p = if temp > 0.0 { ((w - wmax) / temp).exp() } else { w };
            probs[i] = p;
            total += p;
        }
    }
    let mut r = (xorshift(rng) >> 11) as f64 / (1u64 << 53) as f64 * total;
    let mut last = None;
    for (i, &p) in probs[..ws.len()].iter().enumerate() {
        if p > 0.0 {
            last = Some(i);
            r -= p;
            if r < 0.0 {
                return Some(i);
            }
        }
    }
    last
}

/// Stochastic play-out seeded by `seed` (deterministic for a given seed).
pub fn play_out_stochastic(b: &mut Board, temp: f64, seed: u64, cap: usize) -> (usize, bool) {
    let mut rng = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut n = 0usize;
    loop {
        if cap > 0 && n >= cap {
            return (n, false);
        }
        let Some(m) = choose_stochastic(b, temp, &mut rng) else { return (n, true) };
        b.play(m).unwrap();
        n += 1;
    }
}
