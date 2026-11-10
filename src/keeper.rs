//! Pipe keeper (session 7): play that keeps all four pipes working, found without rollouts.
//!
//! A pipe is two columns. Its *stack* is the run of point blocks standing on the floor. A
//! stack is worth the number of separate explosions it will produce if nothing is put
//! underneath it (`events`): clusters whose countdowns meet on the way down count once.
//! After an explosion the pipe is *due*: the two rows above the stack have to become a new
//! cluster before the next explosion, or the pipe is one level shallower for good. A pipe
//! has `events` levels, plus one while it is due, and at most 8.
//!
//! Two searches use that count:
//!
//! - `run`, the keeper beam: a beam over all legal moves that keeps the positions with the
//!   most levels in total. It is what found the 3550, 3588 and 3898 games. Nothing in it
//!   knows about wide clusters, unison or how a pipe gains a level; lines that do those
//!   things simply keep more levels than lines that do not. `KCfg::w_sync` adds a small
//!   bonus for explosions that two neighbouring pipes will share.
//! - `keep`, the planner: depth-first search for a line along which no pipe loses a level.
//!   It holds a tidy position as long as the arithmetic allows (see the research log) and
//!   fails within about 100 moves on positions from the rollout searches' games, which
//!   always carry clusters in the wrong place.
//!
//! The games this produces burn capacity on purpose. A board with more levels than the
//! split count needs gets more splits back than it can spend, and it has no room to store
//! them, so it turns some 2x2 or 4x2 blocks into clusters of 2x1 blocks, which costs 4
//! splits each time. That is the price of having the levels when the count catches up.

use crate::engine::{Board, EMPTY, H, W};
use rayon::prelude::*;

#[derive(Clone, Copy, Default, Debug)]
pub struct PipeView {
    /// Height of the stack in rows.
    pub rows: u8,
    /// Separate explosions the stack will produce if left alone.
    pub events: u8,
    /// The top cluster was made since the pipe's last explosion (it has never been halved).
    pub fresh: bool,
    /// Not fresh, and the two rows above the stack hold free blocks only.
    pub due: bool,
    /// Splits still needed to cut those two rows into 1x1 blocks (an estimate when a block
    /// reaches beyond them).
    pub work: u8,
    /// Moves until the stack's next explosion (0 for an empty stack).
    pub deadline: u16,
    /// Point cells above the stack, and free cells underneath a point cell.
    pub junk: u8,
}

/// Number of separate explosion events a stack of cluster values (bottom to top) produces
/// when no new cluster is put under it. Follows the engine: every move counts every value
/// down by one, a value that runs out explodes, everything above the lowest explosion falls
/// and is halved (rounding down, 1 stays 1).
pub fn events(vals: &[u16]) -> u8 {
    let mut s = [0u16; H];
    let mut k = vals.len();
    s[..k].copy_from_slice(vals);
    let mut ev = 0u8;
    while k > 0 {
        let m = *s[..k].iter().min().unwrap();
        let mut j = 0;
        let mut fell = false;
        for i in 0..k {
            let r = s[i] - m;
            if r == 0 {
                fell = true;
                continue;
            }
            s[j] = if fell && r > 1 { r / 2 } else { r };
            j += 1;
        }
        k = j;
        ev += 1;
    }
    ev
}

/// The stack values of pipe `p`, bottom to top, merging rows of equal value. Returns the
/// number of values and the stack height in rows.
pub fn stack(b: &Board, p: usize, vals: &mut [u16; H]) -> (usize, usize) {
    let (x0, x1) = (2 * p, 2 * p + 1);
    let (mut k, mut rows) = (0usize, 0usize);
    for y in (0..H).rev() {
        let (ia, ib) = (b.grid[y * W + x0], b.grid[y * W + x1]);
        if ia == EMPTY || ib == EMPTY {
            break;
        }
        let (pa, pb) = (b.blocks[ia as usize].pts, b.blocks[ib as usize].pts);
        if pa == 0 || pa != pb {
            break;
        }
        rows += 1;
        if k == 0 || vals[k - 1] != pa {
            vals[k] = pa;
            k += 1;
        }
    }
    (k, rows)
}

pub fn view(b: &Board, p: usize) -> PipeView {
    let mut vals = [0u16; H];
    let (k, rows) = stack(b, p, &mut vals);
    let n = b.moves as u32;
    let mut v = PipeView { rows: rows as u8, events: events(&vals[..k]), ..Default::default() };
    if k > 0 {
        v.deadline = *vals[..k].iter().min().unwrap();
        v.fresh = vals[k - 1] as u32 * 10 > n * 7;
    }
    let (x0, x1) = (2 * p, 2 * p + 1);
    // Junk above the stack.
    for x in [x0, x1] {
        let mut seen_point = false;
        for y in 0..H - rows {
            let id = b.grid[y * W + x];
            if id == EMPTY {
                continue;
            }
            if b.blocks[id as usize].pts > 0 {
                seen_point = true;
                v.junk += 1;
            } else if seen_point {
                v.junk += 1;
            }
        }
    }
    // The slot: the two rows above the stack.
    if !v.fresh && rows + 2 <= H {
        let (y0, y1) = (H - rows - 2, H - rows - 1);
        let mut ids = [EMPTY; 4];
        let mut ok = true;
        for (i, (x, y)) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].into_iter().enumerate() {
            let id = b.grid[y * W + x];
            if id == EMPTY || b.blocks[id as usize].pts > 0 {
                ok = false;
                break;
            }
            ids[i] = id;
        }
        if ok {
            v.due = true;
            let mut work = 0u32;
            for i in 0..4 {
                if ids[..i].contains(&ids[i]) {
                    continue;
                }
                let inside = ids.iter().filter(|&&q| q == ids[i]).count() as u32;
                let area = b.blocks[ids[i] as usize].area();
                // Splits to cut the block down to the part inside the slot, then to 1x1s.
                work += (area / inside).trailing_zeros() + inside - 1;
            }
            v.work = work as u8;
        }
    }
    v
}

/// Would this position (just reached by a move that made a cluster) have an unclean pipe:
/// a new cluster that does not stand on the stack, or one with a point block above it?
/// `n` is the split count of the move. A cluster that fell during the move is not checked.
pub fn unclean_after(t: &Board) -> bool {
    unclean_in(t, 0b1111)
}

/// `unclean_after` for the pipes in `mask` only (bit p = pipe p): a new cluster that lies
/// entirely in other pipes is not looked at.
pub fn unclean_in(t: &Board, mask: u8) -> bool {
    let n = t.moves;
    for bl in t.blocks() {
        if bl.pts != n {
            continue;
        }
        if (bl.x / 2..=(bl.x + bl.w - 1) / 2).all(|p| mask >> p & 1 == 0) {
            continue;
        }
        for x in bl.x as usize..(bl.x + bl.w) as usize {
            for y in (bl.y + bl.h) as usize..H {
                let id = t.grid[y * W + x];
                if id == EMPTY || t.blocks[id as usize].pts == 0 {
                    return true;
                }
            }
            for y in 0..bl.y as usize {
                let id = t.grid[y * W + x];
                // (The other blocks of the new cluster are worth n as well.)
                if id != EMPTY && t.blocks[id as usize].pts > 0 && t.blocks[id as usize].pts != n {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Clone, Copy, Debug)]
pub struct Score {
    /// Total levels: per pipe `events`, plus one for a due pipe (at most 8 per pipe).
    pub phi: u32,
    pub per_pipe: [u8; 4],
    /// Work that cannot be finished before its deadline, over the due pipes taken in
    /// deadline order (parity and wide clusters ignored).
    pub over: u32,
    pub junk: u32,
    pub cap: u32,
    /// The capacity in hand cannot reach the next explosion.
    pub doomed: bool,
    /// Sum over the pipes of 2^levels - 1: the rate at which the pipes return splits, in
    /// units of 3 splits per (split count / 265) moves. The game needs about
    /// split count / 3.118 of it.
    pub rate: u32,
    /// Explosions that two neighbouring pipes will have on the same move (their holes then
    /// refill as one block four wide, worth 7 splits instead of 6).
    pub matched: u32,
    /// The same count with partial credit: an explosion of a pipe that comes d < 8 moves
    /// from the nearest explosion of its right-hand neighbour counts (8 - d) / 8. Two pipes
    /// a few moves apart can be brought together over several intervals by making the later
    /// one's clusters earlier, and this is what rewards each step of that.
    pub near: f32,
}

pub fn score(b: &Board) -> Score {
    let mut s = Score { phi: 0, per_pipe: [0; 4], over: 0, junk: 0, cap: 0, doomed: false, rate: 0, matched: 0, near: 0.0 };
    let mut due: [(u16, u8); 4] = [(0, 0); 4];
    let mut nd = 0;
    let mut ev = [[0u16; H]; 4];
    let mut ne = [0usize; 4];
    for p in 0..4 {
        let v = view(b, p);
        let lv = (v.events + v.due as u8).min(8);
        s.per_pipe[p] = lv;
        s.phi += lv as u32;
        s.rate += (1u32 << lv) - 1;
        ne[p] = event_times(b, p, &mut ev[p]);
        s.junk += v.junk as u32;
        if v.due && v.deadline > 0 {
            due[nd] = (v.deadline, v.work);
            nd += 1;
        }
    }
    for p in 0..3 {
        for &a in &ev[p][..ne[p]] {
            let d = ev[p + 1][..ne[p + 1]].iter().map(|&c| a.abs_diff(c)).min().unwrap_or(u16::MAX);
            if d < 8 {
                s.near += (8 - d) as f32 / 8.0;
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < ne[p] && j < ne[p + 1] {
            match ev[p][i].cmp(&ev[p + 1][j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    s.matched += 1;
                    i += 1;
                    j += 1;
                }
            }
        }
    }
    due[..nd].sort();
    let mut cum = 0u32;
    for &(d, w) in &due[..nd] {
        cum += w as u32;
        s.over += cum.saturating_sub(d as u32);
    }
    // Capacity, and the most horizontal and vertical splits the free blocks can give.
    let (mut max_h, mut max_v) = (0u32, 0u32);
    for x in b.blocks().iter().filter(|x| x.pts == 0) {
        s.cap += x.area() - 1;
        max_h += x.w as u32 * (x.h as u32 - 1);
        max_v += x.h as u32 * (x.w as u32 - 1);
    }
    // To reach the next explosion, e more moves have to be made, alternating from the
    // direction that is due now.
    s.doomed = match b.next_explosion() {
        None => true,
        Some(e) => {
            let e = e as u32;
            let first = e.div_ceil(2);
            let (need_h, need_v) = if b.horizontal_next() { (first, e - first) } else { (e - first, first) };
            s.cap < e || max_h < need_h || max_v < need_v
        }
    };
    s
}

#[derive(Clone, Debug)]
pub struct KCfg {
    pub width: usize,
    /// Largest share of the beam that positions with the same burnt capacity may take.
    pub share: f32,
    /// Score weights: per level, per unit of overdue work, per junk cell.
    pub w_phi: f32,
    pub w_over: f32,
    pub w_junk: f32,
    pub w_cap: f32,
    /// Weight of the return rate (`Score::rate`), counted up to (1 + `margin`) times what the
    /// game needs at this split count; rate beyond that is charged `w_excess` per unit. A
    /// pipe that is deeper than the split count can pay for has to burn its surplus, and
    /// below about 2,400 splits four full pipes cannot do that fast enough.
    pub w_rate: f32,
    pub w_excess: f32,
    pub margin: f32,
    /// Weight per explosion that two neighbouring pipes will share (`Score::matched`).
    pub w_sync: f32,
    /// Weight of `Score::near`.
    pub w_near: f32,
    /// Weight of the capacity bound on the moves left (`search::passive_bound`): how long
    /// the clusters already on the board can carry the game. Late in a game this is the
    /// number of moves left, to the move.
    pub w_bound: f32,
    /// Allow moves that make an unclean cluster (they are always allowed where a position
    /// has no other move).
    pub loose: bool,
    pub seed: u64,
    /// Print a progress line every this many moves (0 = never).
    pub log: usize,
    /// Stop at this split count and return the best-scoring line (0 = play to the end).
    pub until: usize,
}

impl Default for KCfg {
    fn default() -> Self {
        KCfg { width: 500, share: 0.4, w_phi: 1000.0, w_over: 40.0, w_junk: 5.0, w_cap: 0.0, w_rate: 0.0, w_excess: 0.0, margin: 0.3, w_sync: 0.0, w_near: 0.0, w_bound: 0.0, loose: false, seed: 0, log: 50, until: 0 }
    }
}

struct Node {
    b: Board,
    /// Index into the arena of (parent, move).
    at: u32,
    burnt: u32,
    sc: f32,
    doomed: bool,
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

pub struct KOut {
    /// Moves from `start` to the end of the best line.
    pub moves: Vec<u8>,
    pub splits: u16,
    /// Split count at which the total level count last equalled its starting value.
    pub held_until: u16,
}

/// Run the keeper beam from `start`. Returns the longest game found (or, with `until`, the
/// best-scoring line at that split count).
pub fn run(start: &Board, cfg: &KCfg) -> KOut {
    let mut arena: Vec<(u32, u8)> = vec![(u32::MAX, 0)];
    let s0 = score(start);
    let mut beam = vec![Node { b: start.clone(), at: 0, burnt: 0, sc: 0.0, doomed: s0.doomed }];
    let mut best_end: (u16, u32) = (start.moves, 0);
    let mut held_until = start.moves;
    if cfg.log > 0 {
        eprintln!("keeper start at {}: levels {:?} = {}, cap {}", start.moves, s0.per_pipe, s0.phi, s0.cap);
    }
    loop {
        let step = beam[0].b.moves as usize;
        // Expand.
        let kids: Vec<(Board, u32, u8, u32, f32, bool, bool)> = beam
            .par_iter()
            .flat_map_iter(|nd| {
                let mut legal = Vec::new();
                nd.b.legal_moves(&mut legal);
                let mut out = Vec::with_capacity(legal.len());
                let mut held = Vec::new();
                for &m in &legal {
                    let mut t = nd.b.clone();
                    let info = t.play(m as usize).unwrap();
                    let unclean = info.new_point_blocks > 0 && info.exploded == 0 && unclean_after(&t);
                    let s = score(&t);
                    let noise = (mix(t.hash64() ^ cfg.seed.wrapping_mul(0x9E3779B97F4A7C15)) % 1000) as f32 / 1000.0;
                    let allowed = t.moves as f32 / 3.118 * (1.0 + cfg.margin);
                    let rate = s.rate as f32;
                    let sc = cfg.w_phi * s.phi as f32 + cfg.w_rate * rate.min(allowed) - cfg.w_excess * (rate - allowed).max(0.0) + cfg.w_sync * s.matched as f32
                        + cfg.w_near * s.near
                        + if cfg.w_bound > 0.0 { cfg.w_bound * crate::search::passive_bound(&t, 600).0 as f32 } else { 0.0 }
                        - cfg.w_over * s.over as f32
                        - cfg.w_junk * s.junk as f32
                        + cfg.w_cap * s.cap as f32
                        + noise;
                    let kid = (t, nd.at, m, nd.burnt + info.new_point_cap, sc, s.doomed, false);
                    if unclean && !cfg.loose {
                        held.push(kid);
                    } else {
                        out.push(kid);
                    }
                }
                if out.is_empty() {
                    out = held;
                }
                for k in out.iter_mut() {
                    k.6 = k.0.is_over();
                }
                out
            })
            .collect();
        if kids.is_empty() {
            break;
        }
        // Finished games.
        let mut live: Vec<usize> = Vec::with_capacity(kids.len());
        for (i, k) in kids.iter().enumerate() {
            if k.6 {
                if k.0.moves > best_end.0 {
                    arena.push((k.1, k.2));
                    best_end = (k.0.moves, arena.len() as u32 - 1);
                }
            } else {
                live.push(i);
            }
        }
        if live.is_empty() {
            break;
        }
        // Select: not doomed first, then by score; at most `share` of the beam per burnt value.
        live.sort_by(|&a, &b| (kids[a].5, -kids[a].4).partial_cmp(&(kids[b].5, -kids[b].4)).unwrap());
        let cap_per = ((cfg.width as f32 * cfg.share) as usize).max(1);
        let mut seen = std::collections::HashSet::with_capacity(live.len());
        let mut per: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut chosen: Vec<usize> = Vec::with_capacity(cfg.width);
        let mut spill: Vec<usize> = Vec::new();
        for &i in &live {
            if chosen.len() >= cfg.width {
                break;
            }
            if !seen.insert(kids[i].0.hash64()) {
                continue;
            }
            let c = per.entry(kids[i].3).or_insert(0);
            if *c >= cap_per {
                spill.push(i);
                continue;
            }
            *c += 1;
            chosen.push(i);
        }
        for i in spill {
            if chosen.len() >= cfg.width {
                break;
            }
            chosen.push(i);
        }
        let mut next = Vec::with_capacity(chosen.len());
        let mut kids = kids;
        for i in chosen {
            let k = std::mem::replace(&mut kids[i], (Board::new(), 0, 0, 0, 0.0, true, true));
            arena.push((k.1, k.2));
            next.push(Node { b: k.0, at: arena.len() as u32 - 1, burnt: k.3, sc: k.4, doomed: k.5 });
        }
        beam = next;
        let top = score(&beam[0].b);
        if top.phi >= s0.phi {
            held_until = beam[0].b.moves;
        }
        if cfg.log > 0 && (step + 1) % cfg.log == 0 {
            let (bmin, bmax) = beam.iter().fold((u32::MAX, 0), |(lo, hi), x| (lo.min(x.burnt), hi.max(x.burnt)));
            eprintln!(
                "keeper {:>5}: beam {:>4} levels {:?} = {} over {} cap {} burnt {}..{} doomed {} best end {}",
                step + 1,
                beam.len(),
                top.per_pipe,
                top.phi,
                top.over,
                top.cap,
                bmin,
                bmax,
                beam.iter().filter(|x| x.doomed).count(),
                best_end.0
            );
        }
        if cfg.until > 0 && step + 1 >= cfg.until {
            best_end = (beam[0].b.moves, beam[0].at);
            break;
        }
    }
    let _ = beam.first().map(|x| x.sc);
    let mut moves = Vec::new();
    let mut at = best_end.1;
    while at != 0 && at != u32::MAX {
        let (p, m) = arena[at as usize];
        moves.push(m);
        at = p;
    }
    moves.reverse();
    KOut { moves, splits: best_end.0, held_until }
}

// ---------------------------------------------------------------------------------------
// Planner: depth-first search for a line along which no pipe loses a level.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PCfg {
    /// Moves planned ahead.
    pub horizon: usize,
    /// Moves played from each plan before planning again.
    pub commit: usize,
    /// Node budget per search.
    pub nodes: usize,
    /// Largest capacity one plan may burn in clusters of blocks that are not 1x1.
    pub max_burn: u32,
    /// Keep pairs of neighbouring pipes that explode on the same moves doing so.
    pub sync: bool,
    pub seed: u64,
    pub log: usize,
    /// Stop planning at this split count (0 = until no plan exists).
    pub until: usize,
    /// Upper limit on the levels the planner holds per pipe (0 = the pipe is left to chance,
    /// and may hold clusters that are not on its stack).
    pub cap_levels: [u8; 4],
    /// A game to follow: at every position the planner tries the move this game made at the
    /// same split count first (block geometry per split count, as `rollout_beam::guide_of`).
    pub guide: Option<std::sync::Arc<Vec<[u8; 4]>>>,
    /// Try to add a level to a pipe that has room for a second cluster in one interval
    /// (node budget of such an attempt; 0 = never try).
    pub gain_nodes: usize,
    /// Extra moves a level-gaining plan looks ahead.
    pub gain_extra: usize,
}

impl Default for PCfg {
    fn default() -> Self {
        PCfg { horizon: 24, commit: 6, nodes: 300_000, max_burn: 12, sync: false, seed: 0, log: 50, until: 0, cap_levels: [8; 4], guide: None, gain_nodes: 0, gain_extra: 12 }
    }
}

/// Levels per pipe as the planner counts them: `events`, plus one for a due pipe.
pub fn levels(b: &Board) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (p, o) in out.iter_mut().enumerate() {
        let v = view(b, p);
        *o = (v.events + v.due as u8).min(8);
    }
    out
}

/// Times (moves from now) of the explosions the stack of pipe `p` will produce if left alone.
pub fn event_times(b: &Board, p: usize, out: &mut [u16; H]) -> usize {
    let mut s = [0u16; H];
    let (mut k, _) = stack(b, p, &mut s);
    let (mut n, mut t) = (0usize, 0u16);
    while k > 0 {
        let m = *s[..k].iter().min().unwrap();
        t += m;
        let mut j = 0;
        let mut fell = false;
        for i in 0..k {
            let r = s[i] - m;
            if r == 0 {
                fell = true;
                continue;
            }
            s[j] = if fell && r > 1 { r / 2 } else { r };
            j += 1;
        }
        k = j;
        out[n] = t;
        n += 1;
    }
    n
}

/// Do pipes `p` and `p + 1` explode on the same moves for as far as both stacks reach
/// (at least `min` explosions)?
pub fn in_sync(b: &Board, p: usize, min: usize) -> bool {
    let (mut a, mut c) = ([0u16; H], [0u16; H]);
    let (na, nc) = (event_times(b, p, &mut a), event_times(b, p + 1, &mut c));
    let n = na.min(nc);
    n >= min && a[..n] == c[..n]
}

/// If the top clusters of pipes `p` and `p + 1` were both made since their last explosion,
/// will they explode on the same move? (True when either is not fresh.) Two pipes whose
/// new levels always agree stay in unison, and their holes refill as one 4x2 block.
pub fn tops_agree(b: &Board, p: usize) -> bool {
    let (va, vc) = (view(b, p), view(b, p + 1));
    if !va.fresh || !vc.fresh {
        return true;
    }
    let (mut a, mut c) = ([0u16; H], [0u16; H]);
    let (na, nc) = (event_times(b, p, &mut a), event_times(b, p + 1, &mut c));
    na > 0 && nc > 0 && a[na - 1] == c[nc - 1]
}

/// The move the guide made at this split count, if that block exists here and can be split.
fn guide_move(b: &Board, guide: &[[u8; 4]]) -> Option<u8> {
    let [x, y, w, h] = *guide.get(b.moves as usize)?;
    if x == 255 {
        return None;
    }
    let id = b.grid[y as usize * W + x as usize];
    if id == EMPTY {
        return None;
    }
    let bl = &b.blocks[id as usize];
    (bl.x == x && bl.y == y && bl.w == w && bl.h == h && b.can_split(id as usize)).then_some(id)
}

struct Dfs<'a> {
    cfg: &'a PCfg,
    /// Pipes in which every new cluster must stand on the stack.
    clean: u8,
    /// Levels each pipe must hold at the end of the plan (at least `floors`).
    goal: [u8; 4],
    /// A pipe whose blocks are tried first (the one that is to gain a level).
    focus: Option<usize>,
    floors: [u8; 4],
    sync: [bool; 3],
    end: u16,
    failed: std::collections::HashSet<u64>,
    nodes: usize,
    path: Vec<u8>,
    /// Longest line seen (for reporting when no plan exists).
    deepest: Vec<u8>,
}

impl Dfs<'_> {
    fn rec(&mut self, b: &Board, burn_left: u32) -> bool {
        if b.moves >= self.end {
            let lv = levels(b);
            if (0..4).any(|p| lv[p] < self.goal[p]) {
                return false;
            }
            // A pipe that is gaining a level: its two close clusters will explode on
            // consecutive moves, and the cluster that has to be made on the second of those
            // moves needs a horizontal split (an odd split count) if it is to be a wide one
            // shared with the neighbouring pipe.
            if let Some(p) = self.focus {
                let mut ev = [0u16; H];
                let n = event_times(b, p, &mut ev);
                for i in 1..n {
                    if ev[i] == ev[i - 1] + 1 && (b.moves + ev[i]) % 2 == 0 {
                        return false;
                    }
                }
            }
            return true;
        }
        let key = b.hash64() ^ (burn_left as u64).wrapping_mul(0x9E3779B97F4A7C15);
        if self.failed.contains(&key) {
            return false;
        }
        if self.nodes >= self.cfg.nodes {
            return false;
        }
        let mut legal = Vec::new();
        b.legal_moves(&mut legal);
        let gm = self.cfg.guide.as_ref().and_then(|g| guide_move(b, g));
        // Children that satisfy the constraints, with an ordering key.
        let mut kids: Vec<(u32, u8, Board, u32)> = Vec::with_capacity(legal.len());
        for &m in &legal {
            let mut t = b.clone();
            let info = t.play(m as usize).unwrap();
            self.nodes += 1;
            // What the guide's own move burns is not charged to the budget.
            let burn = if gm == Some(m) { 0 } else { info.new_point_cap };
            if burn > burn_left {
                continue;
            }
            if info.new_point_blocks > 0 && info.exploded == 0 && unclean_in(&t, self.clean) {
                continue;
            }
            let s = score(&t);
            if s.doomed && t.moves < self.end {
                continue;
            }
            if (0..4).any(|p| s.per_pipe[p] < self.floors[p]) {
                continue;
            }
            if info.new_point_blocks > 0 && (0..3).any(|p| self.sync[p] && !tops_agree(&t, p)) {
                continue;
            }
            // Prefer no burn, then less overdue work, then more levels; noise breaks ties.
            let noise = (mix(t.hash64() ^ self.cfg.seed.wrapping_mul(0x9E3779B97F4A7C15)) % 16) as u32;
            // The guide's move first; then no burn, less overdue work, more levels.
            let off_focus = self.focus.is_some_and(|f| b.blocks[m as usize].x as usize / 2 != f) as u32;
            let key = if gm == Some(m) { 0 } else { 1 << 28 | (info.new_point_cap.min(15) << 20) | (s.over.min(255) << 12) | off_focus << 11 | ((40 - s.phi.min(40)) << 4) | noise };
            kids.push((key, m, t, burn));
        }
        kids.sort_by_key(|k| k.0);
        for (_, m, t, burn) in kids {
            self.path.push(m);
            if self.path.len() > self.deepest.len() {
                self.deepest = self.path.clone();
            }
            if self.rec(&t, burn_left - burn) {
                return true;
            }
            self.path.pop();
        }
        if self.nodes < self.cfg.nodes {
            self.failed.insert(key);
        }
        false
    }
}

/// Outcome of one planning call.
pub struct Plan {
    pub moves: Vec<u8>,
    /// The constraints held to the horizon.
    pub complete: bool,
    pub nodes: usize,
    pub burn_budget: u32,
}

/// Search for `cfg.horizon` moves from `b` along which every pipe keeps `floors` levels.
/// Tries a burn budget of 0 first, then 4, 8, ... up to `cfg.max_burn`.
pub fn plan(b: &Board, floors: [u8; 4], sync: [bool; 3], cfg: &PCfg) -> Plan {
    plan_goal(b, floors, floors, sync, cfg, cfg.nodes, cfg.max_burn)
}

/// `plan` with levels to reach by the end of the horizon (`goal`), a node budget and a
/// largest burn budget of its own.
pub fn plan_goal(b: &Board, floors: [u8; 4], goal: [u8; 4], sync: [bool; 3], cfg: &PCfg, nodes: usize, max_burn: u32) -> Plan {
    let focus = (0..4).find(|&p| goal[p] > floors[p]);
    // A plan that is to gain a level looks further ahead, so that it includes what the
    // other pipes need right after the extra cluster.
    let horizon = cfg.horizon + if focus.is_some() { cfg.gain_extra } else { 0 };
    let cfg = &PCfg { nodes, max_burn, horizon, ..cfg.clone() };
    let mut total = 0;
    let mut deepest: Vec<u8> = Vec::new();
    let mut budget = 0;
    loop {
        let clean = (0..4).fold(0u8, |m, p| m | ((cfg.cap_levels[p] > 0) as u8) << p);
        let mut d = Dfs { cfg, clean, goal, focus, floors, sync, end: b.moves + cfg.horizon as u16, failed: Default::default(), nodes: 0, path: Vec::new(), deepest: Vec::new() };
        let ok = d.rec(b, budget);
        total += d.nodes;
        if ok {
            return Plan { moves: d.path, complete: true, nodes: total, burn_budget: budget };
        }
        if d.deepest.len() > deepest.len() {
            deepest = d.deepest;
        }
        if budget >= cfg.max_burn {
            return Plan { moves: deepest, complete: false, nodes: total, burn_budget: budget };
        }
        budget += 4;
    }
}

pub struct KeepOut {
    pub moves: Vec<u8>,
    /// Split count at which planning stopped (no plan, or `until`).
    pub stopped: u16,
    /// Levels given up along the way: (split count, pipe).
    pub losses: Vec<(u16, u8)>,
    pub burnt: u32,
    /// Plans for which the unison rule had to be dropped.
    pub unsynced: u32,
    /// Levels gained on purpose: (split count, pipe).
    pub gains: Vec<(u16, u8)>,
}

/// Play from `start` by planning: keep every pipe at its number of levels for as long as a
/// plan exists, give up one level in one pipe when none does, and stop when even that does
/// not help.
pub fn keep(start: &Board, cfg: &PCfg) -> KeepOut {
    let mut b = start.clone();
    let mut floors = levels(&b);
    for p in 0..4 {
        floors[p] = floors[p].min(cfg.cap_levels[p]);
    }
    let mut out = KeepOut { moves: Vec::new(), stopped: b.moves, losses: Vec::new(), burnt: 0, unsynced: 0, gains: Vec::new() };
    let mut sync = [false; 3];
    if cfg.sync {
        for (p, s) in sync.iter_mut().enumerate() {
            *s = in_sync(&b, p, 5);
        }
    }
    if cfg.log > 0 {
        eprintln!("keep start at {}: levels {:?}, sync {:?}", b.moves, floors, sync);
    }
    let mut next_log = b.moves as usize + cfg.log.max(1);
    // A level that has been given up but not lost yet (the floor must not rise meanwhile).
    let mut owed = [false; 4];
    // Earliest split count for the next attempt to gain a level, per pipe.
    let mut gain_wait = [0u16; 4];
    loop {
        if cfg.until > 0 && b.moves as usize >= cfg.until {
            break;
        }
        // Levels can rise (two clusters that will stay distinct); keep what is there.
        let now = levels(&b);
        for p in 0..4 {
            if now[p] <= floors[p] {
                owed[p] = false;
            } else if !owed[p] {
                floors[p] = now[p].min(cfg.cap_levels[p]);
            }
        }
        // A level to gain? A pipe with room for two clusters above its stack can make both in
        // one interval; if they will still be distinct at the bottom the pipe counts one
        // level more from then on, and the planner has to keep it.
        if cfg.gain_nodes > 0 {
            let mut order: Vec<usize> = (0..4).filter(|&p| floors[p] < cfg.cap_levels[p] && !owed[p] && b.moves >= gain_wait[p]).collect();
            order.sort_by_key(|&p| floors[p]);
            let mut gained = false;
            for p in order {
                let v = view(&b, p);
                if (v.rows as usize) + 4 > H || v.junk > 0 {
                    continue;
                }
                let mut goal = floors;
                goal[p] += 1;
                let g = plan_goal(&b, floors, goal, sync, cfg, cfg.gain_nodes, cfg.max_burn);
                if !g.complete {
                    if std::env::var("SPLT_DBG").is_ok() {
                        eprintln!("  gain {:>5} pipe {p}: no plan ({} nodes, deepest {})", b.moves, g.nodes, g.moves.len());
                    }
                    gain_wait[p] = b.moves + 6;
                    continue;
                }
                // The plan up to the move that makes the level; it is only taken if a full
                // plan with the higher floor exists from there.
                let mut t = b.clone();
                let mut upto = 0;
                for (i, &m) in g.moves.iter().enumerate() {
                    t.play(m as usize).unwrap();
                    upto = i + 1;
                    if levels(&t)[p] >= goal[p] {
                        break;
                    }
                }
                if levels(&t)[p] < goal[p] || !plan(&t, goal, sync, cfg).complete {
                    if std::env::var("SPLT_DBG").is_ok() {
                        eprintln!("  gain {:>5} pipe {p}: level made at {} (levels {:?}) but no plan after it", b.moves, t.moves, levels(&t));
                    }
                    gain_wait[p] = b.moves + 6;
                    continue;
                }
                for &m in &g.moves[..upto] {
                    let info = b.play(m as usize).unwrap();
                    out.burnt += info.new_point_cap;
                    out.moves.push(m);
                }
                out.gains.push((b.moves, p as u8));
                if cfg.log > 0 {
                    eprintln!("keep {:>5}: pipe {p} gains a level: levels {:?}", b.moves, levels(&b));
                }
                gained = true;
                break;
            }
            if gained {
                continue;
            }
        }
        let mut pl = plan(&b, floors, sync, cfg);
        let mut tries = 0;
        while !pl.complete && tries < 8 {
            tries += 1;
            // Give up something: first unison (for this plan only), then one level in one
            // pipe (the plan that then exists with the smallest burn budget wins; ties go to
            // the shallowest pipe). If no single level is enough, the shallowest pipe loses
            // one and the search is repeated.
            if sync.iter().any(|&x| x) {
                let p2 = plan(&b, floors, [false; 3], cfg);
                if p2.complete {
                    out.unsynced += 1;
                    pl = p2;
                    break;
                }
            }
            let mut order: Vec<usize> = (0..4).filter(|&p| floors[p] > 0).collect();
            order.sort_by_key(|&p| floors[p]);
            let Some(&shallowest) = order.first() else { break };
            let mut best: Option<(Plan, usize)> = None;
            for &p in &order {
                let mut f2 = floors;
                f2[p] -= 1;
                let p2 = plan(&b, f2, sync, cfg);
                if p2.complete && best.as_ref().is_none_or(|x| p2.burn_budget < x.0.burn_budget) {
                    best = Some((p2, p));
                }
            }
            let lost = match best {
                Some((p2, p)) => {
                    pl = p2;
                    p
                }
                None => shallowest,
            };
            floors[lost] -= 1;
            owed[lost] = true;
            out.losses.push((b.moves, lost as u8));
            if cfg.log > 0 {
                eprintln!("keep {:>5}: gives up a level in pipe {lost}: floors {:?}{}", b.moves, floors, if pl.complete { "" } else { " (still no plan)" });
            }
        }
        if !pl.complete {
            if cfg.log > 0 {
                eprintln!("keep {:>5}: no plan (levels {:?}, floors {:?}, cap {}), stopping", b.moves, now, floors, score(&b).cap);
            }
            break;
        }
        let take = pl.moves.len().min(cfg.commit.max(1));
        for &m in &pl.moves[..take] {
            let info = b.play(m as usize).unwrap();
            out.burnt += info.new_point_cap;
            out.moves.push(m);
        }
        if cfg.log > 0 && b.moves as usize >= next_log {
            next_log += cfg.log;
            eprintln!(
                "keep {:>5}: levels {:?} floors {:?} cap {} burnt {} (last plan {} nodes, burn budget {})",
                b.moves,
                levels(&b),
                floors,
                score(&b).cap,
                out.burnt,
                pl.nodes,
                pl.burn_budget
            );
        }
        if b.is_over() {
            break;
        }
    }
    out.stopped = b.moves;
    out
}
