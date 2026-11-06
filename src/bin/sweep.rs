//! Compare evaluation configurations on a fixed benchmark, using every core.
//!
//! usage: sweep <configs-file> [--width N] [--seq FILE] [--starts 0,500,700,900]
//!
//! Each non-empty, non-# line of <configs-file> holds evaluation flags, as for `solve`
//! (e.g. `--wslot 0.2` or `--wmodel 1 --mh 500`). Every config runs a beam search from each
//! start position: move 0 is a fresh game; k > 0 is the position after the first k moves of
//! --seq (default: Josh Holder's 1208 record game). All (config, start) jobs run in parallel.
//! Score = total splits gained across the starts. For start k > 0, compare with the record's
//! own continuation (len(seq) - k), which is printed as the "record" line.
use rayon::prelude::*;
use splt::engine::{parse_moves, Board};
use splt::search::{arg, beam_search, Params};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg_path = args.get(1).expect("usage: sweep <configs-file> [--width N] [--seq FILE] [--starts 0,500,...]");
    let width: usize = arg(&args, "--width").unwrap_or(200);
    let seq_path: String =
        arg(&args, "--seq").unwrap_or_else(|| "reference/josh-holder-SPLT/sequences/1208seq.txt".to_string());
    let starts: Vec<usize> = arg::<String>(&args, "--starts")
        .unwrap_or_else(|| "0,500,700,900".to_string())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();

    let configs: Vec<String> = std::fs::read_to_string(cfg_path)
        .expect("read configs")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let seq: Vec<u8> = if starts.iter().any(|&k| k > 0) {
        parse_moves(&std::fs::read_to_string(&seq_path).expect("read --seq")).into_iter().map(|m| m as u8).collect()
    } else {
        Vec::new()
    };
    let start_boards: Vec<(usize, Board)> = starts
        .iter()
        .map(|&k| {
            let mut b = Board::new();
            for &m in &seq[..k] {
                b.play(m as usize).unwrap();
            }
            (k, b)
        })
        .collect();

    let record: Vec<String> = starts.iter().map(|&k| if k == 0 { "-".into() } else { format!("{}", seq.len() - k) }).collect();
    println!("width {width}; starts {starts:?}; record continuation per start: [{}]", record.join(", "));

    let jobs: Vec<(usize, usize)> = (0..configs.len()).flat_map(|c| (0..starts.len()).map(move |s| (c, s))).collect();
    let t0 = Instant::now();
    let results: Vec<(usize, usize, u16)> = jobs
        .par_iter()
        .map(|&(c, si)| {
            let cargs: Vec<String> = configs[c].split_whitespace().map(String::from).collect();
            let p = Params::from_args(&cargs);
            let (k, board) = &start_boards[si];
            let r = beam_search(board, &seq[..*k], width, &p, false);
            (c, si, r.splits)
        })
        .collect();

    let mut rows: Vec<(i64, String)> = Vec::new();
    for (c, cfg) in configs.iter().enumerate() {
        let mut gained = 0i64;
        let mut cells = Vec::new();
        for (si, &k) in starts.iter().enumerate() {
            let s = results.iter().find(|r| r.0 == c && r.1 == si).unwrap().2;
            gained += s as i64 - k as i64;
            cells.push(format!("{}@{k}", s));
        }
        rows.push((gained, format!("gained {gained:>5}  [{}]  {cfg}", cells.join(" "))));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, line) in rows {
        println!("{line}");
    }
    println!("({} jobs in {:.1}s)", jobs.len(), t0.elapsed().as_secs_f64());
}
