//! Ratchet: repeatedly re-search the tail of the best game found so far, keeping any
//! result that is at least as long. Never gets worse; meant to run for hours.
//!
//! usage: ratchet --start FILE [--hours H] [--minback B0] [--maxback B1] [--seed0 S]
//!                [--out DIR] [--plateau] + any rbeam search flag (--width, --k, --rollout,
//!                --stoch, --maxkids, --rankpen, ...; --seed is set per iteration)
//!
//! Each iteration cuts the current best game at `len - back` moves, with `back` drawn
//! log-uniformly from [B0, min(B1, len)], and runs one rollout beam from there with a fresh
//! seed. Short cuts are cheap and polish the ending; long cuts are expensive and can move
//! the game to a different line. A strictly longer result becomes the new best and is
//! written to DIR/best_<splits>.txt (and DIR/best.txt); with --plateau an equally long but
//! different game also replaces it, which lets the search drift along ties.
//! Prints one RATCHET line per iteration and a NEWBEST line per improvement.
use splt::engine::parse_moves;
use splt::rollout_beam::{arg, run, verify, write_sequence, Config};
use std::io::Write;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start_path: String = arg(&args, "--start").expect("--start FILE");
    let hours: f64 = arg(&args, "--hours").unwrap_or(8.0);
    let min_back: usize = arg(&args, "--minback").unwrap_or(40);
    let max_back: usize = arg(&args, "--maxback").unwrap_or(1500);
    let mut seed: u64 = arg(&args, "--seed0").unwrap_or(1_000_001);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs/ratchet".to_string());
    let plateau = args.iter().any(|a| a == "--plateau");
    let base = Config::from_args(&args);
    std::fs::create_dir_all(&out).ok();

    let mut best: Vec<u8> = parse_moves(&std::fs::read_to_string(&start_path).expect("read start")).iter().map(|&m| m as u8).collect();
    let mut best_len = verify(&best).expect("start sequence is not a legal finished game") as usize;
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{out}/ratchet.log")).unwrap();
    writeln!(log, "# start {start_path} ({best_len} splits), {}", base.describe()).unwrap();
    println!("RATCHET start {start_path} splits={best_len} {}", base.describe());
    let t0 = Instant::now();
    let deadline = hours * 3600.0;
    let mut rng: u64 = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut iter = 0u64;
    while t0.elapsed().as_secs_f64() < deadline {
        iter += 1;
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let u = (rng >> 11) as f64 / (1u64 << 53) as f64;
        let hi = max_back.min(best_len).max(min_back + 1) as f64;
        let back = (min_back as f64 * (hi / min_back as f64).powf(u)).round() as usize;
        let cut = best_len.saturating_sub(back);
        seed += 1;
        let cfg = Config { seed, noise: if base.noise > 0.0 { base.noise } else { 0.5 }, ..base.clone() };
        let o = run(&best[..cut], &cfg, None, false);
        let improved = o.splits as usize > best_len || (plateau && o.splits as usize == best_len && o.moves != best);
        let line = format!(
            "RATCHET iter {iter} cut {cut} back {back} seed {seed} -> {} (best {best_len}) {:.1}s total {:.0}s",
            o.splits,
            o.secs,
            t0.elapsed().as_secs_f64()
        );
        writeln!(log, "{line}").unwrap();
        log.flush().ok();
        if improved {
            assert_eq!(verify(&o.moves), Some(o.splits), "ratchet result failed verification");
            let gained = o.splits as usize > best_len;
            best = o.moves;
            best_len = o.splits as usize;
            write_sequence(&format!("{out}/best.txt"), &best);
            if gained {
                write_sequence(&format!("{out}/best_{best_len}.txt"), &best);
                let msg = format!("NEWBEST {best_len} at iter {iter} (cut {cut}, seed {seed}) after {:.0}s -> {out}/best_{best_len}.txt", t0.elapsed().as_secs_f64());
                println!("{msg}");
                writeln!(log, "{msg}").unwrap();
                log.flush().ok();
            }
        }
    }
    println!("RATCHET done best={best_len} iters={iter} time={:.0}s -> {out}/best.txt", t0.elapsed().as_secs_f64());
}
