//! Beam-search solver.
//!
//! usage: solve [--width N] [--horizon H] [--ws X] [--wd X] [--tau X] [--wc X] [--wi X]
//!              [--prefix FILE --keep K] [--out DIR] [--quiet]
//!
//! --prefix/--keep: start from the first K moves of an existing sequence (to re-search an
//! endgame). Writes the best sequence to <out>/beam_<splits>_w<width>.txt
use splt::engine::{parse_moves, Board};
use splt::search::{beam_search, Params};
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: usize = arg(&args, "--width").unwrap_or(200);
    let d = Params::default();
    let p = Params {
        horizon: arg(&args, "--horizon").unwrap_or(d.horizon),
        w_survive: arg(&args, "--ws").unwrap_or(d.w_survive),
        w_disc: arg(&args, "--wd").unwrap_or(d.w_disc),
        tau: arg(&args, "--tau").unwrap_or(d.tau),
        w_cap: arg(&args, "--wc").unwrap_or(d.w_cap),
        w_imb: arg(&args, "--wi").unwrap_or(d.w_imb),
        w_debt: arg(&args, "--wdebt").unwrap_or(d.w_debt),
        w_slot: arg(&args, "--wslot").unwrap_or(d.w_slot),
        w_model: arg(&args, "--wmodel").unwrap_or(d.w_model),
        model_horizon: arg(&args, "--mh").unwrap_or(d.model_horizon),
        w_backlog: arg(&args, "--wback").unwrap_or(d.w_backlog),
        alt: args.iter().any(|a| a == "--alt"),
        w_stuck: arg(&args, "--wstuck").unwrap_or(d.w_stuck),
        stuck_t: arg(&args, "--stuckt").unwrap_or(d.stuck_t),
    };
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
    println!("RESULT splits={} width={width} time={secs:.1}s params={p:?} -> {path}", r.splits);
}
