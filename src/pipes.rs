//! Pipe-level structure of a position: the four 2-column "pipes" that strong play settles
//! into, each a stack of clusters whose values should roughly halve from top to bottom.

use crate::engine::{Board, EMPTY, H, W};

/// Point-block values along each pipe, bottom to top, one entry per block (so a 2x2
/// cluster of 1x1 blocks contributes two entries, one per row; use `clusters` for one
/// entry per cluster).
pub fn column_values(b: &Board, col: usize) -> Vec<u16> {
    let mut out = Vec::with_capacity(H);
    let mut last_id = EMPTY;
    for y in (0..H).rev() {
        let id = b.grid[y * W + col];
        if id == EMPTY || id == last_id {
            continue;
        }
        last_id = id;
        let bl = &b.blocks[id as usize];
        if bl.pts > 0 {
            out.push(bl.pts);
        }
    }
    out
}

/// Cluster values along pipe `p` (columns 2p, 2p+1), bottom to top, merging vertically
/// adjacent blocks of equal value (the two rows of a 2x2 cluster of 1x1 blocks).
pub fn clusters(b: &Board, p: usize) -> Vec<u16> {
    let raw = column_values(b, 2 * p);
    let mut out: Vec<u16> = Vec::with_capacity(raw.len());
    for v in raw {
        if out.last() != Some(&v) {
            out.push(v);
        }
    }
    out
}

/// Number of adjacent cluster pairs (within a pipe) whose values are too close: the upper
/// one is less than `ratio` times the lower one. Pairs where the lower value is below
/// `min_v` are ignored (they are about to clear anyway).
pub fn twins(b: &Board, ratio: f64, min_v: u16) -> u32 {
    let mut n = 0;
    for p in 0..4 {
        let c = clusters(b, p);
        for w in c.windows(2) {
            let (lo, hi) = (w[0], w[1]);
            if lo >= min_v && (hi as f64) < ratio * lo as f64 {
                n += 1;
            }
        }
    }
    n
}

/// How far neighbouring pipes are from firing in unison: per pipe pair in `pairs` (bit 0:
/// pipes 0+1, bit 1: pipes 1+2, bit 2: pipes 2+3), the sum over cluster levels (bottom up)
/// of min(1, |a - b| / (2 + 5% of their mean)), with 1 for a level only one pipe has.
/// 0 means identical staircases. Two such pipes explode on the same moves, and their two
/// holes then refill as one 4x2 block worth 7 splits instead of two 2x2 blocks worth 6.
/// The distance shrinks by itself once new clusters go onto both pipes at nearly the same
/// time: halving halves every difference, and an offset between the two bottoms halves and
/// changes sign each cycle.
pub fn unison_dist(b: &Board, pairs: u8) -> f32 {
    let c: [Vec<u16>; 4] = [clusters(b, 0), clusters(b, 1), clusters(b, 2), clusters(b, 3)];
    let mut d = 0.0f32;
    for p in 0..3 {
        if pairs >> p & 1 == 0 {
            continue;
        }
        let n = c[p].len().max(c[p + 1].len());
        for k in 0..n {
            d += match (c[p].get(k), c[p + 1].get(k)) {
                (Some(&x), Some(&y)) => {
                    let (x, y) = (x as f32, y as f32);
                    ((x - y).abs() / (2.0 + 0.025 * (x + y))).min(1.0)
                }
                _ => 1.0,
            };
        }
    }
    d
}
