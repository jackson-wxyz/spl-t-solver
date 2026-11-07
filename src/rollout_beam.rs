//! Rollout beam: beam search that orders moves with Josh Holder's heuristic and evaluates
//! each child by how long a greedy (heuristic) rollout survives from it.
//!
//! Every rollout that reaches the end of the game is a complete legal game, so the best
//! game found is the best (depth + rollout length) over all rollouts, not just the beam's
//! own frontier. The result is reconstructed and replayed from scratch before it is
//! returned.
//!
//! Used by `rbeam` (one search) and `ratchet` (repeated re-search of a best game's tail).

use crate::engine::Board;
use crate::heuristic::{choose, choose_stochastic, play_out, play_out_stochastic, weights, Policy};
use crate::search::{features, Params};
use rayon::prelude::*;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Config {
    /// Beam width.
    pub width: usize,
    /// Children per parent: the top-k heuristic moves (0 = all legal moves).
    pub k: usize,
    /// Rollout cap in moves (0 = play to the end).
    pub cap: usize,
    /// Weight of the rollout end state's passive `survive` feature (tie-break among rollouts
    /// that hit the cap).
    pub tie: f32,
    /// Seed for the score noise and the stochastic rollouts.
    pub seed: u64,
    /// Amplitude of deterministic per-child score noise in [0, noise).
    pub noise: f32,
    /// Extra softmax-sampled rollouts per child; the longest rollout counts.
    pub stoch: usize,
    pub temp: f64,
    /// Deterministic rollout policies; the longest counts.
    pub pols: Vec<Policy>,
    pub wtwin: f32,
    pub twin_ratio: f64,
    pub wstuck: f32,
    pub stuck_t: u16,
    pub wdebt: f32,
    /// At most this many children of one parent survive selection (0 = no limit).
    pub max_kids: usize,
    /// Score penalty per heuristic rank below the top (rank 0 = heuristic's first choice).
    pub rank_pen: f32,
    /// Label of the heuristic factors in use ("default" = Josh Holder's original). The
    /// factors themselves are process-global (`heuristic::set_global`), set by `from_args`.
    pub hparams: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            width: 100,
            k: 4,
            cap: 200,
            tie: 0.01,
            seed: 0,
            noise: 0.0,
            stoch: 0,
            temp: 3.0,
            pols: vec![Policy::Greedy],
            wtwin: 0.0,
            twin_ratio: 1.5,
            wstuck: 0.0,
            stuck_t: 250,
            wdebt: 0.0,
            max_kids: 0,
            rank_pen: 0.0,
            hparams: "default".to_string(),
        }
    }
}

pub fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

impl Config {
    /// Parse the search flags shared by `rbeam` and `ratchet`. `--hparams FILE|name=v,...`
    /// sets the process-wide heuristic factors (call this once per process).
    pub fn from_args(args: &[String]) -> Config {
        let d = Config::default();
        let hparams = match arg::<String>(args, "--hparams") {
            Some(h) => {
                let (text, label) = match std::fs::read_to_string(&h) {
                    Ok(t) => (t, std::path::Path::new(&h).file_stem().unwrap().to_string_lossy().to_string()),
                    Err(_) => (h.clone(), "inline".to_string()),
                };
                crate::heuristic::set_global(crate::heuristic::HParams::parse(&text));
                label
            }
            None => d.hparams.clone(),
        };
        let seed: u64 = arg(args, "--seed").unwrap_or(0);
        let pol_name: String = arg(args, "--policy").unwrap_or_else(|| "greedy".to_string());
        Config {
            width: arg(args, "--width").unwrap_or(d.width),
            k: arg(args, "--k").unwrap_or(d.k),
            cap: arg(args, "--rollout").unwrap_or(d.cap),
            tie: arg(args, "--tie").unwrap_or(d.tie),
            seed,
            noise: arg(args, "--noise").unwrap_or(if seed == 0 { 0.0 } else { 0.5 }),
            stoch: arg(args, "--stoch").unwrap_or(d.stoch),
            temp: arg(args, "--temp").unwrap_or(d.temp),
            pols: match pol_name.as_str() {
                "both" => vec![Policy::Greedy, Policy::AvoidBig],
                s => vec![s.parse().expect("policy")],
            },
            wtwin: arg(args, "--wtwin").unwrap_or(d.wtwin),
            twin_ratio: arg(args, "--twinratio").unwrap_or(d.twin_ratio),
            wstuck: arg(args, "--wstuck").unwrap_or(d.wstuck),
            stuck_t: arg(args, "--stuckt").unwrap_or(d.stuck_t),
            wdebt: arg(args, "--wdebt").unwrap_or(d.wdebt),
            max_kids: arg(args, "--maxkids").unwrap_or(d.max_kids),
            rank_pen: arg(args, "--rankpen").unwrap_or(d.rank_pen),
            hparams,
        }
    }

    pub fn policy_name(&self) -> &'static str {
        match self.pols.as_slice() {
            [Policy::Greedy] => "greedy",
            [Policy::AvoidBig] => "avoidbig",
            _ => "both",
        }
    }

    /// Short description used in file names.
    pub fn tag(&self) -> String {
        let mut t = format!("w{}_k{}_r{}_{}_st{}", self.width, self.k, self.cap, self.policy_name(), self.stoch);
        if self.max_kids > 0 {
            t.push_str(&format!("_mk{}", self.max_kids));
        }
        if self.rank_pen != 0.0 {
            t.push_str(&format!("_rp{}", self.rank_pen));
        }
        if self.hparams != "default" {
            t.push_str(&format!("_h{}", self.hparams));
        }
        t.push_str(&format!("_s{}", self.seed));
        t
    }

    /// All settings, for RESULT lines.
    pub fn describe(&self) -> String {
        format!(
            "width={} k={} rollout={} tie={} policy={} stoch={} temp={} wtwin={} wstuck={} stuckt={} wdebt={} maxkids={} rankpen={} hparams={} seed={} noise={}",
            self.width,
            self.k,
            self.cap,
            self.tie,
            self.policy_name(),
            self.stoch,
            self.temp,
            self.wtwin,
            self.wstuck,
            self.stuck_t,
            self.wdebt,
            self.max_kids,
            self.rank_pen,
            self.hparams,
            self.seed,
            self.noise
        )
    }
}

/// How a rollout was played, so the best one can be replayed exactly at the end.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Det(Policy),
    Stoch(u64),
}

/// Rollout with each deterministic policy plus `stoch` seeded stochastic play-outs; the
/// longest one counts. Returns (moves played, game over?, final board, kind).
fn rollout(start: &Board, cfg: &Config, seed0: u64) -> (usize, bool, Board, Kind) {
    let mut best: Option<(usize, bool, Board, Kind)> = None;
    for &pol in &cfg.pols {
        let mut b = start.clone();
        let (n, over) = play_out(&mut b, pol, cfg.cap);
        if best.as_ref().is_none_or(|x| n > x.0) {
            best = Some((n, over, b, Kind::Det(pol)));
        }
    }
    for j in 0..cfg.stoch {
        let s = seed0.wrapping_add(j as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ 0xD1B54A32D192ED03;
        let mut b = start.clone();
        let (n, over) = play_out_stochastic(&mut b, cfg.temp, s, cfg.cap);
        if best.as_ref().is_none_or(|x| n > x.0) {
            best = Some((n, over, b, Kind::Stoch(s)));
        }
    }
    best.unwrap()
}

fn replay_kind(b: &mut Board, kind: Kind, temp: f64) -> Vec<u8> {
    let mut out = Vec::new();
    match kind {
        Kind::Det(pol) => {
            while let Some(m) = choose(b, pol) {
                b.play(m).unwrap();
                out.push(m as u8);
            }
        }
        Kind::Stoch(seed) => {
            let mut rng = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
            while let Some(m) = choose_stochastic(b, temp, &mut rng) {
                b.play(m).unwrap();
                out.push(m as u8);
            }
        }
    }
    out
}

/// The top-k heuristic moves, best first (k = 0: all legal moves).
pub fn top_k_moves(b: &Board, k: usize) -> Vec<u8> {
    let ws = weights(b);
    let mut idx: Vec<(usize, f64)> = ws.iter().copied().enumerate().filter(|&(_, w)| w > 0.0).collect();
    idx.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if k > 0 {
        idx.truncate(k);
    }
    idx.into_iter().map(|(i, _)| i as u8).collect()
}

#[inline]
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

pub struct Outcome {
    /// Full game: prefix plus the found continuation. Verified legal and finished.
    pub moves: Vec<u8>,
    pub splits: u16,
    pub rollouts: u64,
    pub secs: f64,
}

/// Run one beam search from the position after `prefix`.
///
/// `tree`: optional genealogy dump, one line per level: "L pi,m,score,roll ..." for every
/// node kept at that level. `progress`: print a line every 50 levels to stderr.
pub fn run(prefix: &[u8], cfg: &Config, mut tree: Option<&mut dyn std::io::Write>, progress: bool) -> Outcome {
    let p = Params { stuck_t: cfg.stuck_t, ..Params::default() };
    let mut start = Board::new();
    for &m in prefix {
        start.play(m as usize).expect("prefix contains an illegal move");
    }
    let t0 = Instant::now();
    let mut history: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut frontier: Vec<Board> = vec![start.clone()];
    // Best complete game: (splits, level of the node the rollout started from, node index,
    // kind). Level L node = position after L moves past the start; its rollout is replayed
    // at the end (rollouts are deterministic). If the start is already over, the prefix is
    // the answer.
    let mut best: (u16, usize, u32, Kind) = (start.moves, 0, 0, Kind::Det(Policy::Greedy));
    let mut rollouts: u64 = 0;
    let seed = cfg.seed;

    loop {
        let level = history.len();
        // Generate children (parent, move, rank, hash, board), dedup, evaluate the distinct ones.
        let mut children: Vec<(u32, u8, u8, u64, Board)> = frontier
            .par_iter()
            .enumerate()
            .flat_map_iter(|(pi, node)| {
                let mv = top_k_moves(node, cfg.k);
                mv.into_iter().enumerate().map(move |(r, m)| {
                    let mut c = node.clone();
                    c.play(m as usize);
                    (pi as u32, m, r as u8, c.hash64(), c)
                })
            })
            .collect();
        if children.is_empty() {
            break;
        }
        // Same sort as session 2's rbeam, so earlier seeds reproduce their games exactly.
        children.par_sort_unstable_by_key(|c| c.3);
        children.dedup_by_key(|c| c.3);
        let scored: Vec<(u32, u8, f32, u16, bool, Board, Kind)> = children
            .into_par_iter()
            .map(|(pi, m, r, _, c)| {
                if c.is_over() {
                    return (pi, m, f32::NEG_INFINITY, c.moves, true, c, Kind::Det(Policy::Greedy));
                }
                let seed0 = seed ^ (level as u64) << 40 ^ (pi as u64) << 8 ^ m as u64;
                let (n, over, end, kind) = rollout(&c, cfg, seed0);
                let reached = c.moves + n as u16;
                let mut s = n as f32;
                if !over {
                    s += cfg.tie * features(&end, &p).survive;
                }
                if cfg.wstuck != 0.0 || cfg.wdebt != 0.0 {
                    let f = features(&c, &p);
                    s -= cfg.wstuck * f.stuck + cfg.wdebt * f.debt;
                }
                if cfg.wtwin != 0.0 {
                    s -= cfg.wtwin * crate::pipes::twins(&c, cfg.twin_ratio, 20) as f32;
                }
                if cfg.rank_pen != 0.0 {
                    s -= cfg.rank_pen * r as f32;
                }
                if cfg.noise > 0.0 {
                    let z = mix(seed ^ (level as u64) << 40 ^ (pi as u64) << 8 ^ m as u64);
                    s += cfg.noise * (z >> 40) as f32 / (1u64 << 24) as f32;
                }
                (pi, m, s, reached, over, c, kind)
            })
            .collect();
        rollouts += scored.len() as u64;
        // The child whose rollout produced the longest complete game this level, if it beats
        // the best so far, is forced into the beam so its path stays reconstructible.
        let mut forced: Option<usize> = None;
        for (j, c) in scored.iter().enumerate() {
            if c.4 && c.3 > best.0 && forced.is_none_or(|f| c.3 > scored[f].3) {
                forced = Some(j);
            }
        }
        if let Some(f) = forced {
            best = (scored[f].3, level + 1, 0, scored[f].6); // index fixed below
        }
        let mut live: Vec<(u32, u8, f32, u16, Board)> = Vec::with_capacity(scored.len());
        for (j, (pi, m, s, reached, _over, c, _)) in scored.into_iter().enumerate() {
            if Some(j) == forced {
                live.push((pi, m, f32::INFINITY, reached, c));
            } else if !c.is_over() {
                live.push((pi, m, s, reached, c));
            }
        }
        if live.is_empty() {
            break;
        }
        if cfg.max_kids > 0 {
            // Best first; keep at most max_kids children per parent, up to the width.
            live.par_sort_by(|a, b| b.2.total_cmp(&a.2));
            let mut per_parent = vec![0u16; frontier.len()];
            let mut kept = Vec::with_capacity(cfg.width.min(live.len()));
            for c in live {
                if kept.len() >= cfg.width {
                    break;
                }
                let slot = &mut per_parent[c.0 as usize];
                if (*slot as usize) < cfg.max_kids || c.2 == f32::INFINITY {
                    *slot += 1;
                    kept.push(c);
                }
            }
            live = kept;
        } else if live.len() > cfg.width {
            live.select_nth_unstable_by(cfg.width - 1, |a, b| b.2.total_cmp(&a.2));
            live.truncate(cfg.width);
        }
        if forced.is_some() {
            let idx = live.iter().position(|c| c.2 == f32::INFINITY).expect("forced child kept");
            best.2 = idx as u32;
        }
        if let Some(w) = tree.as_mut() {
            let depth = start.moves as usize + level + 1;
            let mut line = format!("{}", level + 1);
            for &(pi, m, s, reached, _) in &live {
                line.push_str(&format!(" {pi},{m},{:.1},{}", s, reached as i64 - depth as i64));
            }
            line.push('\n');
            w.write_all(line.as_bytes()).unwrap();
        }
        history.push(live.iter().map(|&(pi, m, _, _, _)| (pi, m)).collect());
        frontier = live.into_iter().map(|(_, _, _, _, b)| b).collect();

        if progress && (level + 1) % 50 == 0 {
            eprintln!(
                "  level {:>5} (split {:>5})  beam {:>5}  best game {:>5}  rollouts {:>8}  {:.1}s",
                level + 1,
                start.moves as usize + level + 1,
                frontier.len(),
                best.0,
                rollouts,
                t0.elapsed().as_secs_f64()
            );
        }
    }

    // Reconstruct: path to the best node, then replay its rollout.
    let (splits, blevel, mut idx, bkind) = best;
    let mut rev = Vec::new();
    for l in (0..blevel).rev() {
        let (pi, m) = history[l][idx as usize];
        rev.push(m);
        idx = pi;
    }
    let mut moves = prefix.to_vec();
    moves.extend(rev.into_iter().rev());
    let mut b = Board::new();
    for &m in &moves {
        b.play(m as usize).expect("illegal move in reconstructed path");
    }
    moves.extend(replay_kind(&mut b, bkind, cfg.temp));
    assert!(b.is_over(), "reconstructed game not over");
    assert_eq!(b.moves, splits, "reconstructed game length mismatch");
    Outcome { moves, splits, rollouts, secs: t0.elapsed().as_secs_f64() }
}

/// Write a sequence file in the reference format "[a, b, c]".
pub fn write_sequence(path: &str, moves: &[u8]) {
    let body: Vec<String> = moves.iter().map(|m| m.to_string()).collect();
    std::fs::write(path, format!("[{}]\n", body.join(", "))).expect("write sequence");
}

/// Replay a sequence from scratch and return its length if it is legal and finished.
pub fn verify(moves: &[u8]) -> Option<u16> {
    let mut b = Board::new();
    for &m in moves {
        b.play(m as usize)?;
    }
    if b.is_over() { Some(b.moves) } else { None }
}
