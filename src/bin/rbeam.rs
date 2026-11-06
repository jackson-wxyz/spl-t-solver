//! Rollout beam: beam search that orders moves with Josh Holder's heuristic and evaluates
//! each child by how long a greedy (heuristic) rollout survives from it.
//!
//! usage: rbeam [--width W] [--k K] [--rollout R] [--prefix FILE --keep N] [--out DIR]
//!              [--quiet]
//!
//! --k        children per parent (top-K heuristic moves; 0 = all legal moves)
//! --rollout  rollout cap in moves (0 = play to the end)
//!
//! Every rollout that reaches the end of the game is a complete legal game, so the best
//! game found is the best (depth + rollout length) over all rollouts, not just the beam's
//! own frontier.
use rayon::prelude::*;
use splt::engine::{parse_moves, Board};
use splt::heuristic::{best_move, weights};
use splt::search::{features, Params};
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

/// Greedy rollout. Returns (moves played, game over?, final board).
fn rollout(start: &Board, cap: usize) -> (usize, bool, Board) {
    let mut b = start.clone();
    let mut n = 0usize;
    loop {
        if cap > 0 && n >= cap {
            return (n, false, b);
        }
        let ws = weights(&b);
        let Some(m) = best_move(&ws) else { return (n, true, b) };
        b.play(m).unwrap();
        n += 1;
    }
}

fn top_k_moves(b: &Board, k: usize) -> Vec<u8> {
    let ws = weights(b);
    let mut idx: Vec<(usize, f64)> = ws.iter().copied().enumerate().filter(|&(_, w)| w > 0.0).collect();
    idx.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if k > 0 {
        idx.truncate(k);
    }
    idx.into_iter().map(|(i, _)| i as u8).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: usize = arg(&args, "--width").unwrap_or(100);
    let k: usize = arg(&args, "--k").unwrap_or(4);
    let cap: usize = arg(&args, "--rollout").unwrap_or(300);
    let tie: f32 = arg(&args, "--tie").unwrap_or(0.01);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs".to_string());
    let quiet = args.iter().any(|a| a == "--quiet");
    let p = Params::default();

    let mut prefix: Vec<u8> = Vec::new();
    if let Some(path) = arg::<String>(&args, "--prefix") {
        let all = parse_moves(&std::fs::read_to_string(&path).expect("read prefix"));
        let keep: usize = arg(&args, "--keep").unwrap_or(all.len());
        prefix = all[..keep.min(all.len())].iter().map(|&m| m as u8).collect();
    }
    let mut start = Board::new();
    for &m in &prefix {
        start.play(m as usize).expect("prefix contains an illegal move");
    }
    if !quiet {
        eprintln!("rbeam width {width} k {k} rollout {cap} tie {tie}, starting at split {}", start.moves);
    }
    let t0 = Instant::now();

    struct Node {
        board: Board,
    }
    let mut history: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut frontier = vec![Node { board: start.clone() }];
    // Best complete game: (splits, level of the node the rollout started from, node index).
    // Level L node = position after L moves past the start; its rollout is recomputed later.
    let mut best: (u16, usize, u32) = (start.moves, 0, 0);
    let mut rollouts: u64 = 0;

    loop {
        let level = history.len();
        // Generate children (parent, move) and hashes, dedup, then evaluate the distinct ones.
        let mut children: Vec<(u32, u8, u64, Board)> = frontier
            .par_iter()
            .enumerate()
            .flat_map_iter(|(pi, node)| {
                let mv = top_k_moves(&node.board, k);
                mv.into_iter().map(move |m| {
                    let mut c = node.board.clone();
                    c.play(m as usize);
                    (pi as u32, m, c.hash64(), c)
                })
            })
            .collect();
        if children.is_empty() {
            break;
        }
        children.par_sort_unstable_by_key(|c| c.2);
        children.dedup_by_key(|c| c.2);
        let scored: Vec<(u32, u8, f32, u16, bool, Board)> = children
            .into_par_iter()
            .map(|(pi, m, _, c)| {
                if c.is_over() {
                    return (pi, m, f32::NEG_INFINITY, c.moves, true, c);
                }
                let (n, over, end) = rollout(&c, cap);
                let reached = c.moves + n as u16;
                let mut s = n as f32;
                if !over {
                    // Tie-break among rollouts that hit the cap by the end state's short-term health.
                    s += tie * features(&end, &p).survive;
                }
                (pi, m, s, reached, over, c)
            })
            .collect();
        rollouts += scored.len() as u64;
        // The child whose rollout produced the longest complete game this level, if it beats
        // the best so far, is forced into the beam so its path stays reconstructible.
        let mut forced: Option<usize> = None;
        for (j, c) in scored.iter().enumerate() {
            if c.4 && c.3 > best.0 && forced.is_none_or(|f| c.3 > scored[f].3) {
                forced = Some(j);
            }
        }
        if let Some(f) = forced {
            best = (scored[f].3, level + 1, 0); // index fixed below
        }
        let mut live: Vec<(u32, u8, f32, Board)> = Vec::with_capacity(scored.len());
        for (j, (pi, m, s, _reached, _over, c)) in scored.into_iter().enumerate() {
            if Some(j) == forced {
                live.push((pi, m, f32::INFINITY, c));
            } else if !c.is_over() {
                live.push((pi, m, s, c));
            }
        }
        if live.is_empty() {
            break;
        }
        if live.len() > width {
            live.select_nth_unstable_by(width - 1, |a, b| b.2.total_cmp(&a.2));
            live.truncate(width);
        }
        if forced.is_some() {
            let idx = live.iter().position(|c| c.2 == f32::INFINITY).expect("forced child kept");
            best.2 = idx as u32;
        }
        history.push(live.iter().map(|&(pi, m, _, _)| (pi, m)).collect());
        frontier = live.into_iter().map(|(_, _, _, b)| Node { board: b }).collect();

        if !quiet && (level + 1) % 50 == 0 {
            let top = frontier.len();
            eprintln!(
                "  level {:>5} (split {:>5})  beam {:>5}  best game {:>5}  rollouts {:>8}  {:.1}s",
                level + 1,
                start.moves as usize + level + 1,
                top,
                best.0,
                rollouts,
                t0.elapsed().as_secs_f64()
            );
        }
    }

    // Reconstruct: path to the best node, then replay its greedy rollout.
    let (splits, blevel, mut idx) = best;
    let mut rev = Vec::new();
    for l in (0..blevel).rev() {
        let (pi, m) = history[l][idx as usize];
        rev.push(m);
        idx = pi;
    }
    let mut moves = prefix.clone();
    moves.extend(rev.into_iter().rev());
    let mut b = Board::new();
    for &m in &moves {
        b.play(m as usize).expect("illegal move in reconstructed path");
    }
    loop {
        let ws = weights(&b);
        let Some(m) = best_move(&ws) else { break };
        b.play(m).unwrap();
        moves.push(m as u8);
    }
    assert!(b.is_over(), "reconstructed game not over");
    assert_eq!(b.moves, splits, "reconstructed game length mismatch");
    let secs = t0.elapsed().as_secs_f64();
    std::fs::create_dir_all(&out).ok();
    let path = format!("{out}/rbeam_{}_w{width}_k{k}_r{cap}.txt", b.moves);
    let body: Vec<String> = moves.iter().map(|m| m.to_string()).collect();
    std::fs::write(&path, format!("[{}]\n", body.join(", "))).unwrap();
    println!(
        "RESULT splits={} start={} width={width} k={k} rollout={cap} tie={tie} rollouts={rollouts} time={secs:.1}s -> {path}",
        b.moves, start.moves
    );
}
