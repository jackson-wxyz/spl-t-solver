//! Iterated local search on finished games: kick, polish, keep if not shorter.
//!
//! usage: ils --start FILE[,FILE...] [--hours H] [--out DIR] [--seed0 S]
//!            [--pool N] [--diverse D] [--slack S] [--minback B0] [--maxback B1]
//!            [--pwidth W] [--rounds R] [--neutral P] [--deep B] [--watch DIR[,DIR...]]
//!            [--beam P --widths 100,200 --caps 150,250 --stochs 2 --bminback 120 --bmaxback 400]
//!
//! One iteration: pick a pool game, pick a move `t` between B1 and B0 moves before its end,
//! play a different move there and complete the game by following the original (`kick`),
//! then run guided local search on the result from move t + 1 (`polish`). Most kicks end
//! dozens of moves short; the polish repairs some of them, and a repaired game is sometimes
//! longer than the original. An iteration takes about a second, against 5-50 s for a ratchet
//! attempt, and never needs a rollout of the heuristic alone.
//!
//! A longer game goes into the pool (the N longest games, one per family, as in `ratchet`:
//! two games are one family if they agree up to within D moves of the shorter one's end) and
//! is first polished again from `deep` moves back. A different game of the same length
//! replaces its parent with probability P ("neutral" steps, which let the search drift
//! along a plateau). Parents are drawn from pool members within `slack` of the best.
//! --beam P: with probability P an iteration is instead an ordinary rollout-beam re-search of
//! the tail from a cut (the ratchet's move, with width, rollout cap and stochastic rollouts
//! drawn from the lists), followed by the same polish. Beam attempts cost about ten times a
//! kick but can rebuild a whole tail, which is what a game far from the plateau needs.
//! --watch: every 200 iterations, also read best.txt from these directories into the pool,
//! so several workers can share their finds.
//! Writes DIR/best.txt, DIR/best_<splits>.txt, and DIR/ils.log (one line per iteration that
//! tied or beat its parent, a summary line every 100 iterations, and NEWBEST lines).
use splt::engine::parse_moves;
use splt::rollout_beam::{arg, kick, polish, run, verify, write_sequence, Config};
use std::io::Write;
use std::time::Instant;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

fn read_game(p: &str) -> Option<Vec<u8>> {
    let text = std::fs::read_to_string(p).ok()?;
    if !text.starts_with('[') {
        return None;
    }
    let mv: Vec<u8> = parse_moves(&text).iter().map(|&m| m as u8).collect();
    verify(&mv).map(|_| mv)
}

struct Pool {
    games: Vec<Vec<u8>>, // longest first
    n: usize,
    diverse: usize,
}

impl Pool {
    fn family(&self, mv: &[u8]) -> Option<usize> {
        self.games.iter().position(|m| {
            let cp = m.iter().zip(mv).take_while(|(a, b)| a == b).count();
            cp + self.diverse >= m.len().min(mv.len())
        })
    }
    /// Add a game; returns true if the pool changed.
    fn add(&mut self, mv: Vec<u8>) -> bool {
        match self.family(&mv) {
            Some(fi) if self.games[fi].len() < mv.len() => self.games[fi] = mv,
            Some(_) => return false,
            None => {
                if self.games.len() >= self.n && mv.len() < self.games[self.games.len() - 1].len() {
                    return false;
                }
                self.games.push(mv);
            }
        }
        self.games.sort_by(|a, b| b.len().cmp(&a.len()));
        self.games.truncate(self.n);
        true
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start_paths: String = arg(&args, "--start").expect("--start FILE[,FILE...]");
    let hours: f64 = arg(&args, "--hours").unwrap_or(8.0);
    let out: String = arg(&args, "--out").unwrap_or_else(|| "runs/ils".to_string());
    let seed0: u64 = arg(&args, "--seed0").unwrap_or(1);
    let min_back: usize = arg(&args, "--minback").unwrap_or(75);
    let max_back: usize = arg(&args, "--maxback").unwrap_or(450);
    let slack: usize = arg(&args, "--slack").unwrap_or(10);
    let pwidth: usize = arg(&args, "--pwidth").unwrap_or(100);
    let rounds: usize = arg(&args, "--rounds").unwrap_or(3);
    let neutral: f64 = arg(&args, "--neutral").unwrap_or(0.5);
    let deep: usize = arg(&args, "--deep").unwrap_or(700);
    let watch: Vec<String> = arg::<String>(&args, "--watch").map(|s| s.split(',').map(|x| x.to_string()).collect()).unwrap_or_default();
    let cfg = Config { width: pwidth, ..Config::from_args(&args) };
    let list = |name: &str, d: usize| -> Vec<usize> {
        arg::<String>(&args, name).map(|s| s.split(',').map(|x| x.trim().parse().expect("bad list value")).collect()).unwrap_or_else(|| vec![d])
    };
    let pbeam: f64 = arg(&args, "--beam").unwrap_or(0.0);
    let (widths, caps, stochs) = (list("--widths", 100), list("--caps", 200), list("--stochs", 2));
    let bmin: usize = arg(&args, "--bminback").unwrap_or(120);
    let bmax: usize = arg(&args, "--bmaxback").unwrap_or(400);
    std::fs::create_dir_all(&out).ok();

    let mut pool = Pool { games: Vec::new(), n: arg(&args, "--pool").unwrap_or(8).max(1), diverse: arg(&args, "--diverse").unwrap_or(100) };
    for p in start_paths.split(',') {
        pool.add(read_game(p).unwrap_or_else(|| panic!("{p} is not a legal finished game")));
    }
    let mut best_len = pool.games[0].len();
    write_sequence(&format!("{out}/best.txt"), &pool.games[0]);
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{out}/ils.log")).unwrap();
    let lens = |p: &Pool| p.games.iter().map(|g| g.len().to_string()).collect::<Vec<_>>().join(",");
    writeln!(
        log,
        "# start {start_paths} at {} pool=[{}] diverse={} slack={slack} back={min_back}..{max_back} pwidth={pwidth} rounds={rounds} neutral={neutral} deep={deep} watch={watch:?} beam={pbeam} widths={widths:?} caps={caps:?} stochs={stochs:?} bback={bmin}..{bmax}",
        now(),
        lens(&pool),
        pool.diverse
    )
    .unwrap();
    println!("ILS start splits={best_len} pool=[{}]", lens(&pool));

    let t0 = Instant::now();
    let mut rng: u64 = seed0.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64
    };
    let (mut iter, mut gains, mut ties, mut neutral_steps, mut beams, mut beam_gains) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut sum_diff = 0i64;
    while t0.elapsed().as_secs_f64() < hours * 3600.0 {
        iter += 1;
        if iter % 200 == 0 {
            for d in &watch {
                if let Some(g) = read_game(&format!("{d}/best.txt")) {
                    if pool.add(g) {
                        writeln!(log, "SYNC from {d}: pool=[{}]", lens(&pool)).unwrap();
                    }
                }
            }
            if pool.games[0].len() > best_len {
                best_len = pool.games[0].len();
                write_sequence(&format!("{out}/best.txt"), &pool.games[0]);
            }
        }
        let eligible: Vec<usize> = (0..pool.games.len()).filter(|&i| pool.games[i].len() + slack >= best_len).collect();
        let pi = eligible[((next() * eligible.len() as f64) as usize).min(eligible.len() - 1)];
        let parent = pool.games[pi].clone();
        let plen = parent.len();
        let seed = seed0.wrapping_mul(1_000_003).wrapping_add(iter);
        let ti = Instant::now();
        let is_beam = next() < pbeam;
        let (back, t, klen, mut res);
        if is_beam {
            // Rollout-beam re-search of the tail from a cut, then polish from the cut.
            beams += 1;
            back = (bmin as f64 * (bmax.min(plen - 1).max(bmin + 1) as f64 / bmin as f64).powf(next())).round() as usize;
            t = plen - back.min(plen - 1);
            fn pick(v: &[usize], u: f64) -> usize {
                v[((u * v.len() as f64) as usize).min(v.len() - 1)]
            }
            let bc = Config { seed, noise: 0.5, width: pick(&widths, next()), cap: pick(&caps, next()), stoch: pick(&stochs, next()), ..cfg.clone() };
            let o = run(&parent[..t], &bc, None, false);
            klen = o.moves.len();
            res = polish(&o.moves, t, &cfg, rounds, seed).moves;
        } else {
            back = min_back + (next() * (max_back.min(plen - 1) - min_back + 1) as f64) as usize;
            t = plen - back.min(plen - 1);
            let Some(kicked) = kick(&parent, t, seed, cfg.k, &cfg) else { continue };
            klen = kicked.len();
            // A kick that already ends where the parent does has rejoined it (two moves
            // played in the other order); its neighbourhood is the parent's, so skip it.
            if klen == plen {
                ties += 1;
                continue;
            }
            res = polish(&kicked, t + 1, &cfg, rounds, seed).moves;
        }
        let diff = res.len() as i64 - plen as i64;
        sum_diff += diff;
        if diff > 0 {
            // A longer game: polish it once more from further back before it joins the pool.
            let from = res.len().saturating_sub(deep);
            res = polish(&res, from, &cfg, rounds, seed ^ 0x5bd1e995).moves;
        }
        let rlen = res.len();
        if rlen >= plen && res != parent {
            assert_eq!(verify(&res).map(|x| x as usize), Some(rlen), "ILS result failed verification");
            let what;
            if rlen > plen {
                gains += 1;
                beam_gains += is_beam as u64;
                what = if is_beam { "BEAMGAIN" } else { "GAIN" };
                if pool.family(&res) == Some(pi) || pool.family(&res).is_none() {
                    pool.games[pi] = res.clone();
                    pool.games.sort_by(|a, b| b.len().cmp(&a.len()));
                } else {
                    pool.add(res.clone());
                }
            } else {
                ties += 1;
                if next() < neutral {
                    neutral_steps += 1;
                    what = "NEUTRAL";
                    pool.games[pi] = res.clone();
                } else {
                    what = "TIE";
                }
            }
            writeln!(
                log,
                "ILS iter {iter} {what} parent {plen} back {back} kicked {klen} -> {rlen} (best {best_len}) {:.1}s total {:.0}s at {}",
                ti.elapsed().as_secs_f64(),
                t0.elapsed().as_secs_f64(),
                now()
            )
            .unwrap();
            if rlen > best_len {
                best_len = rlen;
                write_sequence(&format!("{out}/best.txt"), &res);
                write_sequence(&format!("{out}/best_{best_len}.txt"), &res);
                let msg = format!("NEWBEST {best_len} at iter {iter} (kick at {t}, back {back}) after {:.0}s -> {out}/best_{best_len}.txt", t0.elapsed().as_secs_f64());
                println!("{msg}");
                writeln!(log, "{msg}").unwrap();
            }
            log.flush().ok();
        }
        if iter % 100 == 0 {
            writeln!(
                log,
                "STAT iter {iter} gains {gains} ties {ties} neutral {neutral_steps} beams {beams} beam_gains {beam_gains} mean_diff {:.1} best {best_len} pool=[{}] total {:.0}s at {}",
                sum_diff as f64 / iter as f64,
                lens(&pool),
                t0.elapsed().as_secs_f64(),
                now()
            )
            .unwrap();
            log.flush().ok();
        }
    }
    println!("ILS done best={best_len} iters={iter} gains={gains} ties={ties} time={:.0}s pool=[{}] -> {out}/best.txt", t0.elapsed().as_secs_f64(), lens(&pool));
}
