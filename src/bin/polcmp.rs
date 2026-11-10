//! Compare play-out policies by how long they survive from positions of strong games.
//!
//! usage: polcmp --games a.txt,b.txt [--from 2000] [--every 5]
//!
//! Policies: the heuristic's greedy choice, and "chain look-ahead": among the heuristic's
//! top K moves, play the one that leaves the longest capacity chain (`passive_bound`), ties
//! going to the heuristic's order. Prints mean survival per policy and the mean time per
//! play-out move.
use rayon::prelude::*;
use splt::engine::{parse_moves, Board};
use splt::heuristic::{choose_hp, HParams, Policy};
use splt::rollout_beam::{arg, top_k_moves_hp};
use splt::search::passive_bound;
use std::time::Instant;

fn chain_move(b: &Board, k: usize, hp: &HParams) -> Option<usize> {
    let mv = top_k_moves_hp(b, k, hp);
    let mut best: Option<(usize, u16)> = None;
    for &m in &mv {
        let mut c = b.clone();
        c.play(m as usize).unwrap();
        let (bound, _) = passive_bound(&c, 400);
        if best.is_none_or(|(_, bb)| bound > bb) {
            best = Some((m as usize, bound));
        }
    }
    best.map(|(m, _)| m)
}

/// Can some line of play make `d` more moves from here?
fn alive(b: &Board, d: usize) -> bool {
    if d == 0 {
        return true;
    }
    let mut mv = Vec::new();
    b.legal_moves(&mut mv);
    mv.iter().any(|&m| {
        let mut c = b.clone();
        c.play(m as usize).unwrap();
        alive(&c, d - 1)
    })
}

/// The heuristic's best move that does not walk into a forced end within `d` moves (the
/// heuristic's first choice if every move does).
fn safe_move(b: &Board, d: usize, hp: &HParams) -> Option<usize> {
    let mv = top_k_moves_hp(b, 0, hp);
    for &m in &mv {
        let mut c = b.clone();
        c.play(m as usize).unwrap();
        if alive(&c, d) {
            return Some(m as usize);
        }
    }
    mv.first().map(|&m| m as usize)
}

fn survive(start: &Board, pol: usize, hp: &HParams) -> usize {
    let mut b = start.clone();
    let mut n = 0;
    loop {
        let m = match pol {
            0 => choose_hp(&b, Policy::Greedy, hp),
            k if k >= 100 => safe_move(&b, k - 100, hp),
            k => chain_move(&b, k, hp),
        };
        let Some(m) = m else { return n };
        b.play(m).unwrap();
        n += 1;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let from: usize = arg(&args, "--from").unwrap_or(2000);
    let every: usize = arg(&args, "--every").unwrap_or(5);
    let hp = HParams::default();
    println!("{:<44} {:>5} {:>8} {:>8} {:>8} {:>8}", "game", "pos", "greedy", "safe2", "safe4", "safe8");
    let mut tot = [0usize; 4];
    let mut npos = 0usize;
    let mut time = [0f64; 4];
    for f in arg::<String>(&args, "--games").expect("--games").split(',') {
        let moves = parse_moves(&std::fs::read_to_string(f).unwrap_or_else(|_| panic!("read {f}")));
        let mut pos = Vec::new();
        let mut b = Board::new();
        for (k, &m) in moves.iter().enumerate() {
            if k >= from && k + 50 <= moves.len() && (k - from) % every == 0 {
                pos.push(b.clone());
            }
            b.play(m).unwrap();
        }
        let mut row = [0usize; 4];
        for (j, pol) in [0usize, 102, 104, 108].into_iter().enumerate() {
            let t = Instant::now();
            row[j] = pos.par_iter().map(|p| survive(p, pol, &hp)).sum();
            time[j] += t.elapsed().as_secs_f64();
            tot[j] += row[j];
        }
        npos += pos.len();
        let n = pos.len().max(1) as f64;
        println!("{:<44} {:>5} {:>8.1} {:>8.1} {:>8.1} {:>8.1}", f, pos.len(), row[0] as f64 / n, row[1] as f64 / n, row[2] as f64 / n, row[3] as f64 / n);
    }
    let n = npos.max(1) as f64;
    println!("{:<44} {:>5} {:>8.1} {:>8.1} {:>8.1} {:>8.1}", "all", npos, tot[0] as f64 / n, tot[1] as f64 / n, tot[2] as f64 / n, tot[3] as f64 / n);
    println!("wall seconds per policy (all positions, parallel): {:.1} {:.1} {:.1} {:.1}", time[0], time[1], time[2], time[3]);
}
