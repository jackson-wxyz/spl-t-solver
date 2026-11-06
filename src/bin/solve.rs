//! Beam-search solver.
//!
//! usage: solve [--width N] [evaluation flags, see Params::from_args]
//!              [--prefix FILE --keep K] [--out DIR] [--quiet]
//!
//! --prefix/--keep: start from the first K moves of an existing sequence (to re-search an
//! endgame). Writes the best sequence to <out>/beam_<splits>_w<width>.txt
use splt::engine::{parse_moves, Board};
use splt::search::{arg, beam_search, Params};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: usize = arg(&args, "--width").unwrap_or(200);
    let p = Params::from_args(&args);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs".to_string());
    let quiet = args.iter().any(|a| a == "--quiet");

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
        eprintln!("beam width {width}, {p:?}, starting at split {}", start.moves);
    }
    let t0 = Instant::now();
    let r = beam_search(&start, &prefix, width, &p, !quiet);
    let secs = t0.elapsed().as_secs_f64();

    // Verify by replaying from scratch.
    let mut b = Board::new();
    for &m in &r.moves {
        b.play(m as usize).expect("solver produced an illegal move");
    }
    assert!(b.is_over() && b.moves == r.splits, "replay mismatch");

    std::fs::create_dir_all(&out).ok();
    let path = format!("{out}/beam_{}_w{width}.txt", r.splits);
    let body: Vec<String> = r.moves.iter().map(|m| m.to_string()).collect();
    std::fs::write(&path, format!("[{}]\n", body.join(", "))).unwrap();
    println!(
        "RESULT splits={} width={width} time={secs:.1}s evaluated={}/{} children params={p:?} -> {path}",
        r.splits, r.stats.evaluated, r.stats.children
    );
}
