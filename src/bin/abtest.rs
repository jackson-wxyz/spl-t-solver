//! Paired comparison of beam settings on the ratchet's own task: re-search the tail of a
//! game from a cut point. Every attempt (parent game, cut, seed) is run once per arm, so the
//! arms differ only in their settings.
//!
//! usage: abtest --start FILE[,FILE...] [--n 60] [--minback 120] [--maxback 400] [--seed0 S]
//!               --arm "label|rbeam flags" --arm "label|rbeam flags" ...
//!
//! Prints one line per attempt and a summary per arm: mean result minus the parent's length,
//! how often the result ties or beats the parent, the mean gain over the parent (0 when
//! shorter), mean seconds, and the paired difference to the first arm with its standard error.
use splt::engine::parse_moves;
use splt::rollout_beam::{arg, guide_of, run, verify, Config};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let starts: Vec<Vec<u8>> = arg::<String>(&args, "--start")
        .expect("--start FILE[,FILE...]")
        .split(',')
        .map(|p| {
            let mv: Vec<u8> = parse_moves(&std::fs::read_to_string(p).expect("read start")).iter().map(|&m| m as u8).collect();
            verify(&mv).unwrap_or_else(|| panic!("{p} is not a legal finished game"));
            mv
        })
        .collect();
    let n: usize = arg(&args, "--n").unwrap_or(60);
    let min_back: usize = arg(&args, "--minback").unwrap_or(120);
    let max_back: usize = arg(&args, "--maxback").unwrap_or(400);
    let seed0: u64 = arg(&args, "--seed0").unwrap_or(77_000_001);
    let arms: Vec<(String, Config)> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "--arm")
        .map(|(i, _)| {
            let spec = &args[i + 1];
            let (label, flags) = spec.split_once('|').expect("--arm \"label|flags\"");
            let mut fl: Vec<String> = vec!["abtest".to_string()];
            fl.extend(flags.split_whitespace().map(|s| s.to_string()));
            (label.trim().to_string(), Config::from_args(&fl))
        })
        .collect();
    assert!(!arms.is_empty(), "need at least one --arm");

    let mut rng: u64 = seed0.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64
    };
    // diffs[arm][attempt] = result - parent length; secs likewise.
    let mut diffs: Vec<Vec<f64>> = vec![Vec::new(); arms.len()];
    let mut secs: Vec<Vec<f64>> = vec![Vec::new(); arms.len()];
    for i in 0..n {
        let parent = &starts[i % starts.len()];
        let plen = parent.len();
        let hi = max_back.min(plen).max(min_back + 1) as f64;
        let back = (min_back as f64 * (hi / min_back as f64).powf(next())).round() as usize;
        let cut = plen - back;
        let seed = seed0 + i as u64;
        let mut line = format!("ATTEMPT {i} parent {plen} back {back} seed {seed}:");
        for (a, (label, base)) in arms.iter().enumerate() {
            let guide = base.guided.then(|| guide_of(parent));
            let cfg = Config { seed, noise: if base.noise > 0.0 { base.noise } else { 0.5 }, guide, ..base.clone() };
            let o = run(&parent[..cut], &cfg, None, false);
            diffs[a].push(o.splits as f64 - plen as f64);
            secs[a].push(o.secs);
            line.push_str(&format!(" {label} {} ({:.1}s)", o.splits, o.secs));
        }
        println!("{line}");
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let sd = |v: &[f64]| {
        let m = mean(v);
        (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() as f64 - 1.0).max(1.0)).sqrt()
    };
    println!("\n{:<28} {:>8} {:>7} {:>6} {:>6} {:>8} {:>7} {:>16}", "arm", "mean", "sd", "tie+", "gain", "gain+", "secs", "vs first (se)");
    for (a, (label, _)) in arms.iter().enumerate() {
        let d = &diffs[a];
        let ties = d.iter().filter(|&&x| x >= 0.0).count() as f64 / d.len() as f64;
        let gains = d.iter().filter(|&&x| x > 0.0).count() as f64 / d.len() as f64;
        let gplus = mean(&d.iter().map(|&x| x.max(0.0)).collect::<Vec<_>>());
        let paired: Vec<f64> = d.iter().zip(&diffs[0]).map(|(x, y)| x - y).collect();
        let se = sd(&paired) / (paired.len() as f64).sqrt();
        println!(
            "{:<28} {:>8.1} {:>7.1} {:>5.0}% {:>5.0}% {:>8.2} {:>7.1} {:>9.1} ({:.1})",
            label,
            mean(d),
            sd(d),
            100.0 * ties,
            100.0 * gains,
            gplus,
            mean(&secs[a]),
            mean(&paired),
            se
        );
    }
}
