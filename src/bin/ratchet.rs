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
//!
//! --guides 0,1 (v6): with 1, the attempt's rollouts include a guided rollout that follows
//! the parent game wherever the same block still exists (`rollout_beam::play_out_guided`).
//! The beam then searches deviations from the parent along the whole tail, each judged by
//! what it does to the parent's own ending, and the result is never shorter than the parent.
//! --ks, --wchains: per-attempt lists like --widths. --hjit S: multiply each heuristic
//! factor by exp(S * gaussian) for the attempt.
//!
//! --diverse D (v5): a result whose moves agree with a pool member up to within D moves of
//! the shorter game's end belongs to that member's family; it replaces the member only if
//! longer and otherwise is dropped. Without it, the overnight pool of 8 filled with eight
//! 2620-move games that differed only in their last few dozen moves, so every attempt
//! started from the same prefix.
use splt::engine::parse_moves;
use splt::heuristic::NP;
use splt::rollout_beam::{arg, guide_of, run, verify, write_sequence, Config};
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
    let diverse: usize = arg(&args, "--diverse").unwrap_or(0);
    // Index of the pool member whose family `mv` belongs to (identical moves when D = 0).
    let family = |pool: &Vec<(usize, Vec<u8>)>, mv: &[u8]| -> Option<usize> {
        pool.iter().position(|(_, m)| {
            if diverse == 0 {
                m.as_slice() == mv
            } else {
                let cp = m.iter().zip(mv).take_while(|(a, b)| a == b).count();
                cp + diverse >= m.len().min(mv.len())
            }
        })
    };
    let base = Config::from_args(&args);
    let widths: Vec<usize> = list(&args, "--widths").unwrap_or_else(|| vec![base.width]);
    let caps: Vec<usize> = list(&args, "--caps").unwrap_or_else(|| vec![base.cap]);
    let stochs: Vec<usize> = list(&args, "--stochs").unwrap_or_else(|| vec![base.stoch]);
    let ks: Vec<usize> = list(&args, "--ks").unwrap_or_else(|| vec![base.k]);
    let wchains: Vec<f32> = list(&args, "--wchains").unwrap_or_else(|| vec![base.wchain]);
    let guides: Vec<usize> = list(&args, "--guides").unwrap_or_else(|| vec![base.guided as usize]);
    let hjit: f64 = arg(&args, "--hjit").unwrap_or(0.0);
    std::fs::create_dir_all(&out).ok();

    // Pool of (length, moves), longest first.
    let mut pool: Vec<(usize, Vec<u8>)> = Vec::new();
    for p in start_paths.split(',') {
        let mv: Vec<u8> = parse_moves(&std::fs::read_to_string(p).expect("read start")).iter().map(|&m| m as u8).collect();
        let n = verify(&mv).unwrap_or_else(|| panic!("{p} is not a legal finished game")) as usize;
        match family(&pool, &mv) {
            Some(fi) if pool[fi].0 < n => pool[fi] = (n, mv),
            Some(_) => {}
            None => pool.push((n, mv)),
        }
    }
    pool.sort_by(|a, b| b.0.cmp(&a.0));
    pool.truncate(pool_n);
    let mut best_len = pool[0].0;
    write_sequence(&format!("{out}/best.txt"), &pool[0].1);
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{out}/ratchet.log")).unwrap();
    writeln!(
        log,
        "# start {start_paths} ({best_len} splits) at {} pool={pool_n} slack={slack} diverse={diverse} pool_lens={:?} widths={widths:?} caps={caps:?} stochs={stochs:?} ks={ks:?} wchains={wchains:?} guides={guides:?} hjit={hjit} back={min_back}..{max_back} plateau={plateau}, {}",
        now(),
        pool.iter().map(|p| p.0).collect::<Vec<_>>(),
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
        fn pick<T: Copy>(v: &[T], u: f64) -> T {
            v[((u * v.len() as f64) as usize).min(v.len() - 1)]
        }
        let mut hp = base.hp;
        if hjit > 0.0 {
            for i in 0..NP {
                let (u1, u2) = (next().max(1e-12), next());
                let g = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
                hp.s[i] *= (hjit * g).exp();
            }
        }
        let guided = pick(&guides, next()) != 0;
        let cfg = Config {
            seed,
            noise: if base.noise > 0.0 { base.noise } else { 0.5 },
            width: pick(&widths, next()),
            cap: pick(&caps, next()),
            stoch: pick(&stochs, next()),
            k: pick(&ks, next()),
            wchain: pick(&wchains, next()),
            guide: guided.then(|| guide_of(&parent)),
            hp,
            ..base.clone()
        };
        let o = run(&parent[..cut], &cfg, None, false);
        let res = o.splits as usize;
        let line = format!(
            "RATCHET iter {iter} cut {cut} back {back} seed {seed} -> {res} (best {best_len}) {:.1}s total {:.0}s parent {plen} w {} cap {} stoch {} k {} wchain {} guide {} at {}",
            o.secs,
            t0.elapsed().as_secs_f64(),
            cfg.width,
            cfg.cap,
            cfg.stoch,
            cfg.k,
            cfg.wchain,
            guided as u8,
            now()
        );
        writeln!(log, "{line}").unwrap();
        log.flush().ok();
        let fam = if pool_n == 1 { None } else { family(&pool, &o.moves) };
        let accept = if pool_n == 1 {
            pool[0].1 != o.moves && (res > best_len || (plateau && res == best_len))
        } else {
            match fam {
                Some(fi) => res > pool[fi].0,
                None => pool.len() < pool_n || res >= pool[pool.len() - 1].0,
            }
        };
        if accept {
            assert_eq!(verify(&o.moves), Some(o.splits), "ratchet result failed verification");
            if pool_n == 1 {
                pool[0] = (res, o.moves);
            } else {
                match fam {
                    Some(fi) => pool[fi] = (res, o.moves),
                    None => pool.push((res, o.moves)),
                }
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
