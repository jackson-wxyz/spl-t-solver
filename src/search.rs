//! Position evaluation + beam search.
//!
//! Evaluation idea: the game dies when the board runs out of splittable area before the
//! point blocks clear. So for a position we fast-forward the board with *no further splits*
//! (`Board::passive_step`): point blocks count down, explode, blocks above fall and get
//! halved, voids refill. That yields a schedule of when splitting capacity comes back.
//! From it we estimate how many more moves are guaranteed (`survive`), plus a discounted
//! sum of capacity gains (rewarding positions whose point blocks will clear soon - e.g. a
//! new cluster placed on top of a stack that is about to explode and halve it repeatedly).

use crate::engine::Board;
use rayon::prelude::*;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Passive look-ahead horizon, in moves.
    pub horizon: u16,
    /// Weight on the guaranteed-survival estimate (moves).
    pub w_survive: f32,
    /// Weight on discounted future capacity gains.
    pub w_disc: f32,
    /// Discount time constant (moves) for future capacity gains.
    pub tau: f32,
    /// Weight on current splitting capacity.
    pub w_cap: f32,
    /// Weight on horizontal/vertical imbalance (penalty).
    pub w_imb: f32,
    /// Weight on point-block "debt": integral over the look-ahead of point-block area,
    /// divided by board area (penalty). Long-lived clusters (e.g. a fresh high-value
    /// cluster at the bottom of the board, which nothing will ever halve) cost a lot.
    pub w_debt: f32,
    /// Weight on slot quality (penalty): mean over the four 2-column "pipes" of how long a
    /// fresh cluster (value = current split count) placed on top of that pipe would take
    /// to clear, given the explosions already scheduled beneath it.
    pub w_slot: f32,
    /// Weight on the pipeline model's predicted death time (see `pipeline_model`).
    pub w_model: f32,
    /// Look-ahead horizon of the pipeline model, in moves.
    pub model_horizon: u16,
    /// Relative weight of the model's end-of-horizon slack.
    pub w_backlog: f32,
    /// Use direction-aware capacity (`alt_capacity`) instead of raw capacity.
    pub alt: bool,
    /// Weight on "stuck" area (penalty): point-block area that, with no new splits, would
    /// still be on the board `stuck_t` moves from now (e.g. a fresh cluster at the bottom).
    pub w_stuck: f32,
    pub stuck_t: u16,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            horizon: 400,
            w_survive: 1.0,
            w_disc: 0.5,
            tau: 60.0,
            w_cap: 0.3,
            w_imb: 0.5,
            w_debt: 0.0,
            w_slot: 0.2,
            w_model: 0.0,
            model_horizon: 300,
            w_backlog: 0.1,
            alt: false,
            w_stuck: 0.0,
            stuck_t: 250,
        }
    }
}

/// Time for a point block of value `v` to clear if it is halved at each of `events`
/// (sorted times at which something beneath it explodes), counting down 1 per move.
#[inline]
pub fn clear_time(mut v: u32, events: &[u16]) -> u32 {
    let mut t = 0u32;
    for &e in events {
        let e = e as u32;
        if v <= e - t {
            return t + v;
        }
        v -= e - t;
        t = e;
        if v > 1 {
            v /= 2;
        }
    }
    t + v
}

#[inline]
fn point_area(b: &Board) -> u32 {
    b.blocks().iter().filter(|bl| bl.pts > 0).map(|bl| bl.area()).sum()
}

/// (capacity, horizontal-ish capacity, vertical-ish capacity) of the free blocks.
/// capacity = sum(area - 1): every split reduces it by exactly one.
#[inline]
pub fn capacity(b: &Board) -> (i32, i32, i32) {
    let (mut k, mut hs, mut vs) = (0i32, 0i32, 0i32);
    for bl in b.blocks() {
        if bl.pts == 0 {
            k += bl.area() as i32 - 1;
            hs += bl.h as i32 - 1;
            vs += bl.w as i32 - 1;
        }
    }
    (k, hs, vs)
}

/// Direction-aware capacity: how many strictly alternating splits (starting with the
/// current direction) the free blocks can supply, ignoring cluster formation. Greedy: on a
/// horizontal turn split the tallest-relative block, on a vertical turn the widest-relative,
/// keeping the other direction's material intact. Capped at `cap`.
pub fn alt_capacity(b: &Board, cap: u32) -> u32 {
    let mut shapes: [(u8, u8); 160] = [(0, 0); 160];
    let mut n = 0usize;
    for bl in b.blocks() {
        if bl.pts == 0 && (bl.w > 1 || bl.h > 1) {
            shapes[n] = (bl.w, bl.h);
            n += 1;
        }
    }
    let mut horiz = b.horizontal_next();
    let mut count = 0;
    while count < cap {
        let mut best: Option<usize> = None;
        let mut best_key = (0u32, 0u32);
        for (i, &(w, h)) in shapes[..n].iter().enumerate() {
            let ok = if horiz { h > 1 } else { w > 1 };
            if ok {
                // ratio in the needed direction (scaled), then area
                let key = if horiz { ((h as u32) * 16 / w as u32, w as u32 * h as u32) } else { ((w as u32) * 16 / h as u32, w as u32 * h as u32) };
                if best.is_none() || key > best_key {
                    best = Some(i);
                    best_key = key;
                }
            }
        }
        let Some(i) = best else { break };
        let (w, h) = shapes[i];
        let half = if horiz { (w, h / 2) } else { (w / 2, h) };
        shapes[i] = half;
        if (half.0 > 1 || half.1 > 1) && n < shapes.len() {
            shapes[n] = half;
            n += 1;
        }
        if half.0 == 1 && half.1 == 1 {
            shapes[i] = shapes[n - 1];
            n -= 1;
        }
        count += 1;
        horiz = !horiz;
    }
    count
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    pub k0: f32,
    pub survive: f32,
    pub disc: f32,
    pub imbalance: f32,
    pub debt: f32,
    /// Mean over pipes of the clear time of a hypothetical fresh cluster on top.
    pub slot: f32,
    /// Pipeline model: predicted moves until the game dies (capped at model horizon).
    pub death: f32,
    /// Pipeline model: worker time beyond the horizon (spare material = slack).
    pub backlog: f32,
    /// Point-block area still present after `stuck_t` passive moves.
    pub stuck: f32,
}

/// Abstract "pipeline" rollout used as the main evaluation.
///
/// Model: the board is four 2-column pipes. Free area is raw material; turning material of
/// area A into point clusters costs (A - 1) splits and yields A/4 clusters of 1x1 blocks,
/// each placed on top of its pipe with value = split count at creation. A cluster is halved
/// whenever something beneath it in its pipe explodes (`clear_time`). Every explosion
/// refills its pipe from the top, delivering new material. One split happens per move, so
/// the game dies when the worker runs out of material. This captures the feedback loop the
/// real strategy relies on: clusters stacked on top of soon-to-explode stacks clear fast,
/// and their explosions in turn feed and halve the clusters above them.
///
/// `free_area[p]`, `free_cap[p]`: material available now in pipe p.
/// `passive`: (time, exploded area per pipe) for explosions of existing point blocks.
pub fn pipeline_model(n: u32, free_area: [u32; 4], free_cap: [f32; 4], passive: &[(u16, [u8; 4])], horizon: u32) -> (f32, f32) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut heap: BinaryHeap<Reverse<(u32, u8, u32, u32)>> = BinaryHeap::with_capacity(128); // (time, pipe, area, cap*16)
    let mut events: [Vec<u32>; 4] = Default::default();
    for p in 0..4 {
        if free_area[p] > 0 {
            heap.push(Reverse((0, p as u8, free_area[p], (free_cap[p] * 16.0) as u32)));
        }
    }
    for &(t, areas) in passive {
        for p in 0..4 {
            if areas[p] > 0 {
                let a = areas[p] as u32;
                heap.push(Reverse((t as u32, p as u8, a, (a - 1) * 16)));
                events[p].push(t as u32);
            }
        }
    }
    let mut acc = [0f32; 4];
    let mut w = 0f32;
    while let Some(Reverse((t, p, area, cap16))) = heap.pop() {
        if t > horizon {
            break;
        }
        if w < t as f32 {
            return (w, 0.0); // idle: nothing left to split
        }
        let p = p as usize;
        let cap = cap16 as f32 / 16.0;
        acc[p] += area as f32 / 4.0;
        let units = acc[p].floor();
        acc[p] -= units;
        let m = units as u32;
        for j in 0..m {
            let c = (w + cap * (j + 1) as f32 / m as f32).ceil() as u32;
            // halving events strictly after creation, relative to c
            let ev = &events[p];
            let start = ev.partition_point(|&e| e <= c);
            let mut v = n + c;
            let mut tt = c;
            let mut expl = None;
            for &e in &ev[start..] {
                if v <= e - tt {
                    expl = Some(tt + v);
                    break;
                }
                v -= e - tt;
                tt = e;
                if v > 1 {
                    v /= 2;
                }
            }
            let expl = expl.unwrap_or(tt + v);
            if expl <= horizon {
                heap.push(Reverse((expl, p as u8, 4, 3 * 16)));
                let pos = events[p].partition_point(|&e| e <= expl);
                events[p].insert(pos, expl);
            }
        }
        w += cap;
    }
    let death = w.min(horizon as f32);
    (death, (w - horizon as f32).max(0.0))
}

pub fn features(b: &Board, p: &Params) -> Features {
    let mut pipe_events: [[u16; 32]; 4] = [[0; 32]; 4];
    let mut pipe_n = [0usize; 4];
    let mut passive: Vec<(u16, [u8; 4])> = Vec::with_capacity(48);
    let (k0, hs, vs) = capacity(b);
    // Direction-aware usable capacity of the current free blocks.
    let alt = if p.alt { alt_capacity(b, k0.max(0) as u32) as i32 } else { k0 };
    let mut t = b.clone();
    let mut elapsed: u16 = 0;
    let mut kprev = k0;
    let mut cur = alt; // capacity usable by the moves made so far in the look-ahead
    let mut survive: Option<i32> = None;
    let mut disc = 0.0f32;
    let mut debt = 0.0f32;
    let mut pa = point_area(&t);
    let mut stuck: Option<u32> = None;
    while let Some(dt) = t.next_explosion() {
        if stuck.is_none() && elapsed + dt > p.stuck_t {
            stuck = Some(pa);
        }
        if elapsed + dt > p.horizon {
            debt += pa as f32 * (p.horizon - elapsed) as f32;
            break;
        }
        debt += pa as f32 * dt as f32;
        let (_, info) = t.passive_step(elapsed).unwrap();
        elapsed += dt;
        for (pp, (ev, n)) in pipe_events.iter_mut().zip(pipe_n.iter_mut()).enumerate() {
            if info.exploded_cols >> (2 * pp) & 3 != 0 && *n < 32 {
                ev[*n] = elapsed;
                *n += 1;
            }
        }
        let ca = info.exploded_col_area;
        passive.push((elapsed, [ca[0] + ca[1], ca[2] + ca[3], ca[4] + ca[5], ca[6] + ca[7]]));
        pa = point_area(&t);
        // Moves up to `elapsed` can only use capacity `cur`.
        if survive.is_none() && cur < elapsed as i32 {
            survive = Some(cur);
        }
        let (k, _, _) = capacity(&t);
        let gain = k - kprev;
        kprev = k;
        cur += gain;
        disc += gain as f32 * (-(elapsed as f32) / p.tau).exp();
    }
    let survive = survive.unwrap_or(cur.min(p.horizon as i32)) as f32;
    let v0 = b.moves as u32 + 3;
    let slot = (0..4).map(|pp| clear_time(v0, &pipe_events[pp][..pipe_n[pp]]) as f32).sum::<f32>() / 4.0;

    // Free material per pipe (cells per column, summed per column pair).
    let mut col_area = [0u32; 8];
    let mut free_total = 0u32;
    for bl in b.blocks() {
        if bl.pts == 0 {
            for c in bl.x..bl.x + bl.w {
                col_area[c as usize] += bl.h as u32;
            }
            free_total += bl.area();
        }
    }
    let free_area = [col_area[0] + col_area[1], col_area[2] + col_area[3], col_area[4] + col_area[5], col_area[6] + col_area[7]];
    let scale = if free_total > 0 { alt as f32 / free_total as f32 } else { 0.0 };
    let free_cap = free_area.map(|a| a as f32 * scale);
    let (death, backlog) = if p.w_model != 0.0 {
        pipeline_model(b.moves as u32, free_area, free_cap, &passive, p.model_horizon as u32)
    } else {
        (0.0, 0.0)
    };
    let stuck = stuck.unwrap_or(0) as f32;
    Features { k0: k0 as f32, survive, disc, imbalance: (hs - vs).abs() as f32, debt: debt / 128.0, slot, death, backlog, stuck }
}

#[inline]
pub fn evaluate(b: &Board, p: &Params) -> f32 {
    let f = features(b, p);
    p.w_survive * f.survive + p.w_disc * f.disc + p.w_cap * f.k0
        - p.w_imb * f.imbalance
        - p.w_debt * f.debt
        - p.w_slot * f.slot
        + p.w_model * (f.death + p.w_backlog * f.backlog)
        - p.w_stuck * f.stuck
}

pub struct SearchResult {
    /// Full move sequence (block indices, replayable with `replay`).
    pub moves: Vec<u8>,
    pub splits: u16,
}

/// Beam search from `start` (which must be the position after `prefix`).
pub fn beam_search(start: &Board, prefix: &[u8], width: usize, p: &Params, verbose: bool) -> SearchResult {
    struct Node {
        board: Board,
        score: f32,
    }
    let t0 = Instant::now();
    // history[L][i] = (parent index at level L, move) for frontier node i at level L+1.
    let mut history: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut frontier = vec![Node { board: start.clone(), score: 0.0 }];
    // Best finished game: (splits, level of parent, parent index, final move).
    let mut best: Option<(u16, usize, u32, u8)> = None;

    loop {
        let level = history.len();
        let children: Vec<(u32, u8, f32, u64, u16, bool)> = frontier
            .par_iter()
            .enumerate()
            .flat_map_iter(|(pi, node)| {
                let mut mv = Vec::with_capacity(64);
                node.board.legal_moves(&mut mv);
                mv.into_iter().map(move |m| {
                    let mut c = node.board.clone();
                    c.play(m as usize);
                    let term = c.is_over();
                    let s = if term { f32::NEG_INFINITY } else { evaluate(&c, p) };
                    (pi as u32, m, s, c.hash64(), c.moves, term)
                })
            })
            .collect();

        for &(pi, m, _, _, splits, term) in &children {
            if term && best.is_none_or(|b| splits > b.0) {
                best = Some((splits, level, pi, m));
            }
        }

        let mut live: Vec<(u32, u8, f32, u64)> =
            children.into_iter().filter(|c| !c.5).map(|(pi, m, s, h, _, _)| (pi, m, s, h)).collect();
        if live.is_empty() {
            break;
        }
        // Dedup identical positions reached by different move orders.
        live.par_sort_unstable_by_key(|c| c.3);
        live.dedup_by_key(|c| c.3);
        if live.len() > width {
            live.select_nth_unstable_by(width - 1, |a, b| b.2.total_cmp(&a.2));
            live.truncate(width);
        }
        let next: Vec<Node> = live
            .par_iter()
            .map(|&(pi, m, s, _)| {
                let mut c = frontier[pi as usize].board.clone();
                c.play(m as usize);
                Node { board: c, score: s }
            })
            .collect();
        history.push(live.iter().map(|&(pi, m, _, _)| (pi, m)).collect());
        frontier = next;

        if verbose && (level + 1) % 25 == 0 {
            let top = frontier.iter().map(|n| n.score).fold(f32::NEG_INFINITY, f32::max);
            let sample = frontier.iter().max_by(|a, b| a.score.total_cmp(&b.score)).unwrap();
            let f = features(&sample.board, p);
            eprintln!(
                "  splits {:>5}  beam {:>6}  top eval {:>8.1} (survive {:.0}, disc {:.1}, cap {:.0}, imb {:.0}, debt {:.0}, slot {:.0}, death {:.0}, backlog {:.0})  {:.1}s",
                sample.board.moves,
                frontier.len(),
                top,
                f.survive,
                f.disc,
                f.k0,
                f.imbalance,
                f.debt,
                f.slot,
                f.death,
                f.backlog,
                t0.elapsed().as_secs_f64()
            );
        }
    }

    let (splits, level, mut idx, last) = best.expect("search found no finished game");
    let mut rev = vec![last];
    for l in (0..level).rev() {
        let (pi, m) = history[l][idx as usize];
        rev.push(m);
        idx = pi;
    }
    let mut moves = prefix.to_vec();
    moves.extend(rev.into_iter().rev());
    SearchResult { moves, splits }
}
