//! Ratchet: repeatedly re-search the tail of the best games found so far, keeping any
//! result that is at least as long. Never gets worse; meant to run for hours.
//!
//! usage: ratchet --start FILE[,FILE...] [--hours H] [--minback B0] [--maxback B1]
//!                [--seed0 S] [--out DIR] [--plateau] [--pool N] [--slack D]
//!                [--widths 100,200] [--caps 150,200,300] [--stochs 0,2]
//!                + any rbeam search flag (--width, --k, --rollout, --stoch, --maxkids, ...)
//!
//! Each iteration picks a parent game, cuts it at `len - back` moves (`back` drawn
//! log-uniformly from [B0, min(B1, len)]) and runs one rollout beam from there with a fresh
//! seed. With --widths / --caps / --stochs, each iteration also draws its beam width,
//! rollout cap and number of stochastic rollouts from those lists.
//!
//! Pool: the N longest distinct games seen so far (N = 1 by default: just the best). The
//! parent is drawn uniformly from pool members within `slack` moves of the best. A result
//! joins the pool if it is distinct and at least as long as the pool's shortest member
//! (or, with N = 1, strictly longer than the best; with --plateau also equally long).
//! Every new best is written to DIR/best_<splits>.txt and DIR/best.txt. Prints one RATCHET
//! line per iteration (to DIR/ratchet.log) and a NEWBEST line per improvement.
use splt::engine::parse_moves;
use splt::rollout_beam::{arg, run, verify, write_sequence, Config};
use std::io::Write;
use std::time::Instant;

fn list<T: std::str::FromStr>(args: &[String], name: &str) -> Option<Vec<T>> {
    arg::<String>(args, name).map(|s| s.split(',').map(|x| x.trim().parse().ok().expect("bad list value")).collect())
}

fn now() -> String {
    // Seconds since the Unix epoch; the summary scripts convert it.
    format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start_paths: String = arg(&args, "--start").expect("--start FILE[,FILE...]");
    let hours: f64 = arg(&args, "--hours").unwrap_or(8.0);
    let min_back: usize = arg(&args, "--minback").unwrap_or(120);
    let max_back: usize = arg(&args, "--maxback").unwrap_or(600);
    let mut seed: u64 = arg(&args, "--seed0").unwrap_or(1_000_001);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs/ratchet".to_string());
    let plateau = args.iter().any(|a| a == "--plateau");
    let pool_n: usize = arg(&args, "--pool").unwrap_or(1).max(1);
    let slack: usize = arg(&args, "--slack").unwrap_or(0);
    let base = Config::from_args(&args);
    let widths: Vec<usize> = list(&args, "--widths").unwrap_or_else(|| vec![base.width]);
    let caps: Vec<usize> = list(&args, "--caps").unwrap_or_else(|| vec![base.cap]);
    let stochs: Vec<usize> = list(&args, "--stochs").unwrap_or_else(|| vec![base.stoch]);
    std::fs::create_dir_all(&out).ok();

    // Pool of (length, moves), longest first.
    let mut pool: Vec<(usize, Vec<u8>)> = Vec::new();
    for p in start_paths.split(',') {
        let mv: Vec<u8> = parse_moves(&std::fs::read_to_string(p).expect("read start")).iter().map(|&m| m as u8).collect();
        let n = verify(&mv).unwrap_or_else(|| panic!("{p} is not a legal finished game")) as usize;
        if !pool.iter().any(|(_, m)| *m == mv) {
            pool.push((n, mv));
        }
    }
    pool.sort_by(|a, b| b.0.cmp(&a.0));
    pool.truncate(pool_n);
    let mut best_len = pool[0].0;
    write_sequence(&format!("{out}/best.txt"), &pool[0].1);
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{out}/ratchet.log")).unwrap();
    writeln!(
        log,
        "# start {start_paths} ({best_len} splits) at {} pool={pool_n} slack={slack} widths={widths:?} caps={caps:?} stochs={stochs:?} back={min_back}..{max_back} plateau={plateau}, {}",
        now(),
        base.describe()
    )
    .unwrap();
    println!("RATCHET start {start_paths} splits={best_len} {}", base.describe());
    let t0 = Instant::now();
    let deadline = hours * 3600.0;
    let mut rng: u64 = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut iter = 0u64;
    while t0.elapsed().as_secs_f64() < deadline {
        iter += 1;
        // Parent: uniform among pool members within `slack` of the best.
        let eligible: Vec<usize> = (0..pool.len()).filter(|&i| pool[i].0 + slack >= best_len).collect();
        let pi = eligible[((next() * eligible.len() as f64) as usize).min(eligible.len() - 1)];
        let (plen, parent) = (pool[pi].0, pool[pi].1.clone());
        let hi = max_back.min(plen).max(min_back + 1) as f64;
        let back = (min_back as f64 * (hi / min_back as f64).powf(next())).round() as usize;
        let cut = plen.saturating_sub(back);
        seed += 1;
        let pick = |v: &Vec<usize>, u: f64| v[((u * v.len() as f64) as usize).min(v.len() - 1)];
        let cfg = Config {
            seed,
            noise: if base.noise > 0.0 { base.noise } else { 0.5 },
            width: pick(&widths, next()),
            cap: pick(&caps, next()),
            stoch: pick(&stochs, next()),
            ..base.clone()
        };
        let o = run(&parent[..cut], &cfg, None, false);
        let res = o.splits as usize;
        let line = format!(
            "RATCHET iter {iter} cut {cut} back {back} seed {seed} -> {res} (best {best_len}) {:.1}s total {:.0}s parent {plen} w {} cap {} stoch {} at {}",
            o.secs,
            t0.elapsed().as_secs_f64(),
            cfg.width,
            cfg.cap,
            cfg.stoch,
            now()
        );
        writeln!(log, "{line}").unwrap();
        log.flush().ok();
        let distinct = !pool.iter().any(|(_, m)| *m == o.moves);
        let accept = distinct
            && if pool_n == 1 { res > best_len || (plateau && res == best_len) } else { pool.len() < pool_n || res >= pool[pool.len() - 1].0 };
        if accept {
            assert_eq!(verify(&o.moves), Some(o.splits), "ratchet result failed verification");
            if pool_n == 1 {
                pool[0] = (res, o.moves);
            } else {
                pool.push((res, o.moves));
                pool.sort_by(|a, b| b.0.cmp(&a.0));
                pool.truncate(pool_n);
            }
            if res > best_len {
                best_len = res;
                write_sequence(&format!("{out}/best.txt"), &pool[0].1);
                write_sequence(&format!("{out}/best_{best_len}.txt"), &pool[0].1);
                let msg = format!("NEWBEST {best_len} at iter {iter} (cut {cut}, seed {seed}) after {:.0}s -> {out}/best_{best_len}.txt", t0.elapsed().as_secs_f64());
                println!("{msg}");
                writeln!(log, "{msg}").unwrap();
                log.flush().ok();
            } else if pool_n == 1 {
                write_sequence(&format!("{out}/best.txt"), &pool[0].1);
            }
        }
    }
    let lens: Vec<String> = pool.iter().map(|(n, _)| n.to_string()).collect();
    println!("RATCHET done best={best_len} iters={iter} time={:.0}s pool=[{}] -> {out}/best.txt", t0.elapsed().as_secs_f64(), lens.join(","));
}
