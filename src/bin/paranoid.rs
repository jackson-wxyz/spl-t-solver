//! Port of Josh Holder's "ParanoidPlay" (paranoidAlgoPlay.py / randomParanoidPlay.py).
//!
//! Depth-first backtracking over the greedy heuristic: play the best-weighted move; whenever
//! that move creates point blocks bigger than 1x1 (a "big cluster"), or, after move
//! `--tie-from`, the second-best move was within (1, 3) weight of the best, remember the
//! position with the chosen move's weight zeroed and the runner-up promoted. When the game
//! ends, pop the most recent remembered position and continue from there.
//!
//! usage: paranoid [--prefix FILE --keep N] [--random N --seed S] [--games G] [--secs T]
//!                 [--tie-from M] [--out DIR] [--quiet]
//!
//! --random N: start each run with N weighted-random moves (randomParanoidPlay.py); when
//! the backtracking stack is exhausted, reseed and start again (never with --prefix).
//! --tie-from M: enable near-tie decision points from move M (the reference enabled them
//! from move 875 in paranoidAlgoPlay.py and never in randomParanoidPlay.py; default never).
use splt::engine::{parse_moves, Board};
use splt::heuristic::weights;
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

/// Python: zero out every repeated weight, then take the top two (lowest index on ties).
fn dedup_and_top2(ws: &mut [f64]) -> (Option<usize>, Option<usize>) {
    for i in 0..ws.len() {
        if ws[i] != 0.0 && ws[..i].contains(&ws[i]) {
            ws[i] = 0.0;
        }
    }
    let (mut b1, mut b2): (Option<usize>, Option<usize>) = (None, None);
    for (i, &w) in ws.iter().enumerate() {
        if b1.is_none_or(|j| w > ws[j]) {
            b2 = b1;
            b1 = Some(i);
        } else if b2.is_none_or(|j| w > ws[j]) {
            b2 = Some(i);
        }
    }
    (b1, b2)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let max_games: u64 = arg(&args, "--games").unwrap_or(u64::MAX);
    let secs: f64 = arg(&args, "--secs").unwrap_or(f64::INFINITY);
    let tie_from: usize = arg(&args, "--tie-from").unwrap_or(usize::MAX);
    let random_n: usize = arg(&args, "--random").unwrap_or(0);
    let seed: u64 = arg(&args, "--seed").unwrap_or(1);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs".to_string());
    let quiet = args.iter().any(|a| a == "--quiet");
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);

    let mut prefix: Vec<u8> = Vec::new();
    if let Some(path) = arg::<String>(&args, "--prefix") {
        let all = parse_moves(&std::fs::read_to_string(&path).expect("read prefix"));
        let keep: usize = arg(&args, "--keep").unwrap_or(all.len());
        prefix = all[..keep.min(all.len())].iter().map(|&m| m as u8).collect();
    }

    let t0 = Instant::now();
    let mut best_len = 0usize;
    let mut best_seq: Vec<u8> = Vec::new();
    let mut games: u64 = 0;
    let mut stack: Vec<(Board, Vec<u8>, Vec<f64>)> = Vec::new();
    let mut exploring_from = usize::MAX;
    std::fs::create_dir_all(&out).ok();

    'outer: loop {
        // ---- a fresh run: prefix or weighted-random opening --------------------------
        let mut board = Board::new();
        let mut path: Vec<u8> = Vec::new();
        let mut forced: Vec<u8> = prefix.clone();
        if random_n > 0 {
            let mut rb = Board::new();
            let mut ws = vec![1.0f64];
            for _ in 0..random_n {
                if rb.is_over() {
                    break;
                }
                let total: f64 = ws.iter().sum();
                let mut r = rng.unit() * total;
                let mut m = usize::MAX;
                for (i, &w) in ws.iter().enumerate() {
                    r -= w;
                    if r < 0.0 {
                        m = i;
                        break;
                    }
                }
                if m == usize::MAX || !rb.can_split(m) {
                    // Python falls through with move -1 (last box) in this case; just stop.
                    break;
                }
                rb.play(m).unwrap();
                forced.push(m as u8);
                ws = weights(&rb);
            }
        }
        let mut cur_weights: Option<Vec<f64>> = None;

        loop {
            if t0.elapsed().as_secs_f64() > secs || games >= max_games {
                break 'outer;
            }
            let mut ws = cur_weights.take().unwrap_or_else(|| weights(&board));
            let (b1, b2) = dedup_and_top2(&mut ws);
            let game_over = b1.is_none_or(|i| ws[i] == 0.0) || board.is_over();
            if game_over {
                games += 1;
                let n = path.len();
                if n > best_len {
                    best_len = n;
                    best_seq = path.clone();
                    let p = format!("{out}/paranoid_{n}.txt");
                    let body: Vec<String> = best_seq.iter().map(|m| m.to_string()).collect();
                    std::fs::write(&p, format!("[{}]\n", body.join(", "))).unwrap();
                    if !quiet {
                        eprintln!("new record {n} splits after {games} games, {:.1}s -> {p}", t0.elapsed().as_secs_f64());
                    }
                }
                let Some((b, pth, w)) = stack.pop() else {
                    if !quiet {
                        eprintln!("stack exhausted after {games} games (best {best_len})");
                    }
                    if random_n > 0 {
                        continue 'outer;
                    }
                    break 'outer;
                };
                if pth.len() < exploring_from {
                    exploring_from = pth.len();
                    if !quiet {
                        eprintln!("  exploring from {} splits ({games} games, best {best_len}, stack {})", exploring_from, stack.len());
                    }
                }
                board = b;
                path = pth;
                cur_weights = Some(w);
                continue;
            }
            let b1 = b1.unwrap();
            let weight_diff = match b2 {
                Some(j) if ws[j] > 0.0 => ws[b1] - ws[j],
                _ => 100.0,
            };
            let mut mv = b1;
            if path.len() < forced.len() {
                mv = forced[path.len()] as usize;
            }
            let old_board = board.clone();
            let old_path = path.clone();
            board.play(mv).expect("heuristic chose an illegal move");
            path.push(mv as u8);

            let last = board.blocks[board.len as usize - 1];
            let big_cluster = last.pts != 0 && (last.w != 1 || last.h != 1);
            let near_tie = weight_diff < 3.0 && weight_diff > 1.0 && path.len() > 600 && path.len() >= tie_from;
            if (big_cluster || near_tie) && path.len() > forced.len() {
                if let Some(b2) = b2 {
                    let mut old_w = ws.clone();
                    let max_other = old_w.iter().enumerate().filter(|&(i, _)| i != b1).map(|(_, &w)| w).fold(0.0, f64::max);
                    if max_other != 0.0 {
                        old_w[b2] = old_w[b1];
                        old_w[b1] = 0.0;
                        stack.push((old_board, old_path, old_w));
                    }
                }
            }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    println!("RESULT splits={best_len} games={games} time={secs:.1}s stack={} -> {out}/paranoid_{best_len}.txt", stack.len());
}
