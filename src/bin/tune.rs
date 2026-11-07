//! Tune the heuristic's term factors (`heuristic::HParams`) to make greedy play survive
//! longer from strong positions.
//!
//! usage: tune --train a.txt,b.txt --valid c.txt,d.txt [--from 300] [--every 25] [--cap 800]
//!             [--iters 600] [--sigma 0.25] [--seed 1] [--start "name=v,..."] [--out FILE]
//!
//! Positions: every `every` moves from move `from` to 50 moves before the end of each game.
//! Objective: mean greedy rollout length (capped) over the training positions. Optimizer:
//! (1+1) evolution strategy; each step perturbs 1-3 factors with Gaussian noise of scale
//! `sigma` and keeps the candidate if the training mean does not drop. Validation is the
//! same mean over positions from held-out games, reported whenever the best changes.
use rayon::prelude::*;
use splt::engine::{parse_moves, Board};
use splt::heuristic::{play_out_with, HParams, NP, PNAMES};
use splt::rollout_beam::arg;

fn positions(files: &str, from: usize, every: usize) -> Vec<Board> {
    let mut out = Vec::new();
    for f in files.split(',').filter(|s| !s.is_empty()) {
        let moves = parse_moves(&std::fs::read_to_string(f).unwrap_or_else(|_| panic!("read {f}")));
        let mut b = Board::new();
        for (k, &m) in moves.iter().enumerate() {
            if k >= from && k + 50 <= moves.len() && (k - from) % every == 0 {
                out.push(b.clone());
            }
            b.play(m).expect("illegal move");
        }
    }
    out
}

fn score(pos: &[Board], hp: &HParams, cap: usize) -> f64 {
    let total: usize = pos
        .par_iter()
        .map(|p| {
            let mut b = p.clone();
            play_out_with(&mut b, hp, cap).0
        })
        .sum();
    total as f64 / pos.len() as f64
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let train = positions(&arg::<String>(&args, "--train").expect("--train"), arg(&args, "--from").unwrap_or(300), arg(&args, "--every").unwrap_or(25));
    let valid = positions(&arg::<String>(&args, "--valid").unwrap_or_default(), arg(&args, "--from").unwrap_or(300), arg(&args, "--every").unwrap_or(25));
    let cap: usize = arg(&args, "--cap").unwrap_or(800);
    let iters: usize = arg(&args, "--iters").unwrap_or(600);
    let sigma: f64 = arg(&args, "--sigma").unwrap_or(0.25);
    let mut rng: u64 = arg::<u64>(&args, "--seed").unwrap_or(1).wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let out: Option<String> = arg(&args, "--out");
    let mut best = arg::<String>(&args, "--start").map(|t| HParams::parse(&t)).unwrap_or_default();
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64
    };
    let gauss = |u1: f64, u2: f64| (-2.0 * u1.max(1e-12).ln()).sqrt() * (std::f64::consts::TAU * u2).cos();

    let default = HParams::default();
    let d_train = score(&train, &default, cap);
    let d_valid = if valid.is_empty() { 0.0 } else { score(&valid, &default, cap) };
    println!("positions: train {} valid {}; default factors: train {:.1} valid {:.1}", train.len(), valid.len(), d_train, d_valid);
    let mut best_train = score(&train, &best, cap);
    let mut best_valid = if valid.is_empty() { 0.0 } else { score(&valid, &best, cap) };
    println!("start: train {:.1} valid {:.1}  {}", best_train, best_valid, best.describe());
    for it in 1..=iters {
        let mut cand = best;
        let nmut = 1 + (next() * 3.0) as usize;
        for _ in 0..nmut {
            let i = ((next() * NP as f64) as usize).min(NP - 1);
            let (u1, u2) = (next(), next());
            cand.s[i] += sigma * gauss(u1, u2) * best.s[i].abs().max(0.5);
        }
        let t = score(&train, &cand, cap);
        if t >= best_train {
            let improved = t > best_train;
            best = cand;
            best_train = t;
            if improved {
                best_valid = if valid.is_empty() { 0.0 } else { score(&valid, &best, cap) };
                println!("iter {it}: train {:.1} valid {:.1}  {}", best_train, best_valid, best.describe());
                if let Some(o) = &out {
                    std::fs::write(o, format!("{}\n", best.describe())).unwrap();
                }
            }
        }
    }
    println!("FINAL train {:.1} (default {:.1}) valid {:.1} (default {:.1})", best_train, d_train, best_valid, d_valid);
    for (n, v) in PNAMES.iter().zip(best.s.iter()) {
        println!("  {n:>15} {v:.4}");
    }
}
