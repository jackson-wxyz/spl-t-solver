//! Rollout beam search (one run). See `src/rollout_beam.rs` for the algorithm.
//!
//! usage: rbeam [--width W] [--k K] [--rollout R] [--seed S] [--stoch N --temp T]
//!              [--policy greedy|avoidbig|both] [--maxkids M] [--rankpen X]
//!              [--wtwin X] [--wstuck X --stuckt T] [--wdebt X] [--tie X] [--noise X]
//!              [--prefix FILE --keep N] [--tree FILE] [--out DIR] [--quiet]
//!
//! --k        children per parent (top-K heuristic moves; 0 = all legal moves)
//! --rollout  rollout cap in moves (0 = play to the end)
//! --seed     > 0 adds deterministic score noise (default amplitude 0.5) so a setting can
//!            be sampled several times
//! --maxkids  at most M children of one parent survive each selection (beam diversity)
//! --rankpen  score penalty per heuristic rank below first (prefer the heuristic in ties)
//! --tree     dump the beam genealogy (parent index and rollout length of every kept node)
use splt::engine::parse_moves;
use splt::rollout_beam::{arg, run, write_sequence, Config};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = Config::from_args(&args);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs".to_string());
    let quiet = args.iter().any(|a| a == "--quiet");
    let tree_path: Option<String> = arg(&args, "--tree");
    let mut tree_file = tree_path.as_ref().map(|p| std::io::BufWriter::new(std::fs::File::create(p).expect("create tree file")));

    let mut prefix: Vec<u8> = Vec::new();
    if let Some(path) = arg::<String>(&args, "--prefix") {
        let all = parse_moves(&std::fs::read_to_string(&path).expect("read prefix"));
        let keep: usize = arg(&args, "--keep").unwrap_or(all.len());
        prefix = all[..keep.min(all.len())].iter().map(|&m| m as u8).collect();
    }
    if !quiet {
        eprintln!("rbeam {}, starting at split {}", cfg.describe(), prefix.len());
    }
    let o = run(&prefix, &cfg, tree_file.as_mut().map(|w| w as &mut dyn std::io::Write), !quiet);
    std::fs::create_dir_all(&out).ok();
    let path = format!("{out}/rbeam_{}_{}.txt", o.splits, cfg.tag());
    write_sequence(&path, &o.moves);
    println!(
        "RESULT splits={} start={} {} rollouts={} time={:.1}s -> {path}",
        o.splits,
        prefix.len(),
        cfg.describe(),
        o.rollouts,
        o.secs
    );
}
