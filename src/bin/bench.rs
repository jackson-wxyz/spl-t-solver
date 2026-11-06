//! Speed benchmark + random-play baseline.  usage: bench [games]
use splt::engine::Board;
use std::time::Instant;

fn main() {
    let games: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(200_000);
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    let mut moves = Vec::with_capacity(128);
    let t0 = Instant::now();
    let mut total_moves: u64 = 0;
    let mut best = 0;
    let mut hist = [0u64; 32]; // buckets of 50 splits
    for _ in 0..games {
        let mut b = Board::new();
        loop {
            b.legal_moves(&mut moves);
            if moves.is_empty() {
                break;
            }
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let m = moves[(rng % moves.len() as u64) as usize];
            b.play(m as usize);
        }
        total_moves += b.moves as u64;
        best = best.max(b.moves);
        hist[(b.moves as usize / 50).min(31)] += 1;
    }
    let dt = t0.elapsed().as_secs_f64();
    println!(
        "{games} random games, {total_moves} moves in {dt:.2}s = {:.2} M moves/s (single thread); mean {:.1}, best {best}",
        total_moves as f64 / dt / 1e6,
        total_moves as f64 / games as f64
    );
    for (i, c) in hist.iter().enumerate() {
        if *c > 0 {
            println!("  {:>4}-{:<4} {c}", i * 50, i * 50 + 49);
        }
    }
}
