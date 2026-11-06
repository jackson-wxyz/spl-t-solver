//! Josh Holder's heuristic as a player.
//!
//! usage: josh_play                 greedy game from scratch (score + speed)
//!        josh_play --agree <seq>   how often the greedy choice matches a record sequence
//!        josh_play --prefix <seq> --keep K   greedy continuation from move K of <seq>
use splt::engine::{parse_moves, Board};
use splt::heuristic::{best_move, weights};
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(path) = arg::<String>(&args, "--agree") {
        let moves = parse_moves(&std::fs::read_to_string(&path).expect("read"));
        let mut b = Board::new();
        let mut rank_hist = [0usize; 6]; // rank 1..5, 6+ (by weight, lowest index first on ties)
        let mut agree = 0usize;
        let mut per_100 = vec![[0usize; 2]; moves.len() / 100 + 1];
        for (k, &m) in moves.iter().enumerate() {
            if k > 0 {
                let ws = weights(&b);
                // Rank of the record move: number of blocks with strictly greater weight,
                // plus equal-weight blocks with a lower index (the reference dedups equal
                // weights by keeping the first).
                let wm = ws[m];
                let rank = 1 + ws.iter().enumerate().filter(|&(i, &w)| w > wm || (w == wm && i < m)).count();
                rank_hist[(rank - 1).min(5)] += 1;
                if rank == 1 {
                    agree += 1;
                    per_100[k / 100][0] += 1;
                }
                per_100[k / 100][1] += 1;
            }
            b.play(m).expect("illegal");
        }
        println!("{path}: greedy choice matches the record move at {agree}/{} positions", moves.len() - 1);
        println!("rank of record move: 1:{} 2:{} 3:{} 4:{} 5:{} 6+:{}", rank_hist[0], rank_hist[1], rank_hist[2], rank_hist[3], rank_hist[4], rank_hist[5]);
        let s: Vec<String> = per_100.iter().enumerate().filter(|(_, c)| c[1] > 0).map(|(i, c)| format!("{}:{:.0}%", i * 100, 100.0 * c[0] as f64 / c[1] as f64)).collect();
        println!("agreement per 100 moves: {}", s.join(" "));
        return;
    }
    let mut b = Board::new();
    let mut prefix = Vec::new();
    if let Some(path) = arg::<String>(&args, "--prefix") {
        let all = parse_moves(&std::fs::read_to_string(&path).expect("read"));
        let keep: usize = arg(&args, "--keep").unwrap_or(all.len());
        for &m in &all[..keep.min(all.len())] {
            b.play(m).expect("illegal prefix move");
            prefix.push(m);
        }
    }
    let start = b.moves;
    let t0 = Instant::now();
    let mut seq = prefix.clone();
    loop {
        let ws = weights(&b);
        let Some(m) = best_move(&ws) else { break };
        b.play(m).unwrap();
        seq.push(m);
    }
    let dt = t0.elapsed().as_secs_f64();
    println!(
        "greedy from move {start}: {} splits ({} new) in {:.3}s = {:.1} us/move",
        b.moves,
        b.moves - start,
        dt,
        dt * 1e6 / (b.moves - start).max(1) as f64
    );
    if args.iter().any(|a| a == "--print") {
        let s: Vec<String> = seq.iter().map(|m| m.to_string()).collect();
        println!("[{}]", s.join(", "));
    }
}
