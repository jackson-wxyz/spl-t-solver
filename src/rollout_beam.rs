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
use crate::heuristic::{analyse, choose_hp, choose_stochastic_hp, play_out_hp, play_out_stochastic_hp, weights_with, HParams, Policy};
use crate::search::{features, passive_bound, Params};
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
    /// Label of the heuristic factors in use ("default" = Josh Holder's original).
    pub hparams: String,
    /// The heuristic factors used for move ordering and rollouts.
    pub hp: HParams,
    /// Weight of the capacity chain left at the end of a rollout (`search::passive_bound`):
    /// score = rollout length + wchain * bound(end state). A greedy rollout often dies with
    /// capacity it could not use (wrong split direction), which makes its length a noisy
    /// measure late in a game; the chain is what a careful finish could still play.
    pub wchain: f32,
    /// Score penalty per unit of `pipes::unison_dist` (how far neighbouring pipes are from
    /// identical staircases), for the pipe pairs in `sync_pairs` (bit mask, default 0b101:
    /// pipes 0+1 and 2+3). Steers a beam towards pipes that fire in unison.
    pub wsync: f32,
    pub sync_pairs: u8,
    /// For the first `nocluster` moves of the game, only consider moves that do not create a
    /// cluster, as long as one exists (an opening experiment: delay the first point blocks).
    pub nocluster: usize,
    /// Guided rollouts: when the guide's move for this turn is not available, look this many
    /// moves back in the guide (same split direction) for a move that still is, before
    /// falling back to the heuristic. That replays a guide move the path skipped earlier.
    pub gscan: usize,
    /// Guided rollouts use the to-do-list follower (`Todo`) instead of the time-aligned one,
    /// looking at most `gahead` moves ahead in the guide.
    pub gtodo: bool,
    pub gahead: usize,
    /// Reuse the endings of guided rollouts by position (`EndCache`). On by default.
    pub gcache: bool,
    /// `--guide 1`: the caller should set `guide` to the game being re-searched.
    pub guided: bool,
    /// Guide game for guided rollouts (see `play_out_guided`), as the geometry of the block
    /// split at each move number.
    pub guide: Option<Guide>,
}

/// A game as the geometry [x, y, w, h] of the block split at each move (index = split count
/// before the move). Unlike block indices, this still means something on a nearby board.
pub type Guide = std::sync::Arc<Vec<[u8; 4]>>;

pub fn guide_of(moves: &[u8]) -> Guide {
    let mut b = Board::new();
    let mut g = Vec::with_capacity(moves.len());
    for &m in moves {
        let bl = b.blocks[m as usize];
        g.push([bl.x, bl.y, bl.w, bl.h]);
        b.play(m as usize).expect("illegal move in guide");
    }
    std::sync::Arc::new(g)
}

/// The guide's move for this position, if a free block with exactly that geometry exists;
/// failing that, the most recent of the guide's last `scan` same-direction moves that does.
#[inline]
fn guided_move(b: &Board, guide: &[[u8; 4]], scan: usize) -> Option<usize> {
    let t = b.moves as usize;
    if let Some(m) = guide_at(b, guide, t) {
        return Some(m);
    }
    let mut back = 2;
    while back <= scan && back <= t {
        if let Some(m) = guide_at(b, guide, t - back) {
            return Some(m);
        }
        back += 2;
    }
    None
}

#[inline]
fn guide_at(b: &Board, guide: &[[u8; 4]], t: usize) -> Option<usize> {
    let [x, y, w, h] = *guide.get(t)?;
    let id = b.grid[y as usize * crate::engine::W + x as usize];
    if id == crate::engine::EMPTY {
        return None;
    }
    let bl = &b.blocks[id as usize];
    (bl.x == x && bl.y == y && bl.w == w && bl.h == h && b.can_split(id as usize)).then_some(id as usize)
}

/// Which of the guide's moves a line of play has already made, for the to-do follower: bit j
/// of `mask` stands for guide move `base + j`; moves before `base` count as done or dropped.
///
/// The plain follower plays the guide's move for the current split count or nothing. That is
/// brittle: if one refill arrives two moves late, the guide's move for it is not there on
/// time and never comes up again. The to-do follower treats the guide as an ordered list
/// instead: play the earliest guide move (of the current split direction) that has not been
/// made yet and whose block exists now. A late block's move is simply made when the block
/// turns up, and moves whose turn has not come can be made early in its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Todo {
    pub base: u32,
    pub mask: u128,
}

pub const TODO_BEHIND: usize = 40;

impl Todo {
    /// State of a line that has followed the guide exactly for its first `t` moves.
    pub fn at(t: usize) -> Todo {
        let base = t.saturating_sub(TODO_BEHIND);
        let done = t - base;
        Todo { base: base as u32, mask: if done == 0 { 0 } else { (1u128 << done) - 1 } }
    }
    /// Drop moves that have been pending for more than TODO_BEHIND moves.
    #[inline]
    fn advance(&mut self, t: usize) {
        let nb = t.saturating_sub(TODO_BEHIND);
        if nb > self.base as usize {
            let sh = nb - self.base as usize;
            self.mask = if sh >= 128 { 0 } else { self.mask >> sh };
            self.base = nb as u32;
        }
    }
    #[inline]
    fn used(&self, i: usize) -> bool {
        let j = i - self.base as usize;
        j < 128 && self.mask >> j & 1 == 1
    }
    #[inline]
    fn mark(&mut self, i: usize) {
        let j = i - self.base as usize;
        if j < 128 {
            self.mask |= 1u128 << j;
        }
    }
    /// First index >= base with the parity of `t`.
    #[inline]
    fn first(&self, t: usize) -> usize {
        let b = self.base as usize;
        if (b ^ t) & 1 == 0 { b } else { b + 1 }
    }
    /// The earliest pending guide move of the current direction, at most `ahead` moves in
    /// the future, whose block exists now: (block index, guide index).
    pub fn next(&mut self, b: &Board, guide: &[[u8; 4]], ahead: usize) -> Option<(usize, usize)> {
        let t = b.moves as usize;
        self.advance(t);
        let end = (t + ahead + 1).min(guide.len()).min(self.base as usize + 128);
        let mut i = self.first(t);
        while i < end {
            if !self.used(i) {
                if let Some(m) = guide_at(b, guide, i) {
                    return Some((m, i));
                }
            }
            i += 2;
        }
        None
    }
    /// Record that block geometry `g` was split at split count `t`: it uses up the earliest
    /// pending guide move with that geometry, if there is one within `ahead`.
    pub fn consume(&mut self, g: [u8; 4], t: usize, guide: &[[u8; 4]], ahead: usize) {
        self.advance(t);
        let end = (t + ahead + 1).min(guide.len()).min(self.base as usize + 128);
        let mut i = self.first(t);
        while i < end {
            if !self.used(i) && guide[i] == g {
                self.mark(i);
                return;
            }
            i += 2;
        }
    }
    fn key(&self) -> u64 {
        mix(self.base as u64 ^ (self.mask as u64).rotate_left(17) ^ ((self.mask >> 64) as u64).rotate_left(41))
    }
}

/// To-do-list guided rollout (see `Todo`): the earliest pending guide move that is playable,
/// else the heuristic's choice, until the game ends. Returns (moves played, from cache?).
pub fn play_out_todo(b: &mut Board, guide: &[[u8; 4]], mut st: Todo, ahead: usize, hp: &HParams, record: Option<&mut Vec<u8>>, cache: Option<&EndCache>) -> (usize, bool) {
    let start = b.moves;
    let mut rec = record;
    let mut seen: Vec<u64> = Vec::new();
    let (end, hit) = loop {
        if let Some(c) = cache {
            st.advance(b.moves as usize);
            let h = b.hash64() ^ st.key();
            if let Some(end) = c.get(h) {
                break (end, true);
            }
            seen.push(h);
        }
        let m = match st.next(b, guide, ahead) {
            Some((m, i)) => {
                st.mark(i);
                m
            }
            None => match choose_hp(b, Policy::Greedy, hp) {
                Some(m) => {
                    let bl = b.blocks[m];
                    st.consume([bl.x, bl.y, bl.w, bl.h], b.moves as usize, guide, ahead);
                    m
                }
                None => break (b.moves, false),
            },
        };
        b.play(m).unwrap();
        if let Some(r) = rec.as_mut() {
            r.push(m as u8);
        }
    };
    if let Some(c) = cache {
        c.put_all(&seen, end);
    }
    ((end - start) as usize, hit)
}

/// Time-aligned guided rollout without a cache, for analysis tools. Returns moves played.
pub fn play_out_aligned(b: &mut Board, guide: &[[u8; 4]], scan: usize, hp: &HParams) -> usize {
    play_out_guided(b, guide, scan, hp, None, None).0
}

/// Where guided rollouts end, by position: hash of a position -> split count at which the
/// guided rollout through it ended. A deviation that rejoins the guide (for example by
/// playing two of its moves in the other order) reaches a position some earlier rollout has
/// already played out, and can stop there. Values are exact up to one thing: the heuristic
/// breaks exact weight ties by block index, and the same position can be reached with its
/// blocks listed in a different order. So a cached ending that would be a new best game is
/// always replayed before it is believed (see `rollout`).
pub struct EndCache {
    shards: Vec<std::sync::Mutex<std::collections::HashMap<u64, u16, std::hash::BuildHasherDefault<IdHasher>>>>,
}

/// Position hashes are already well mixed.
#[derive(Default)]
pub struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!()
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

const SHARDS: usize = 256;
const SHARD_MAX: usize = 40_000;

impl EndCache {
    pub fn new() -> Self {
        EndCache { shards: (0..SHARDS).map(|_| Default::default()).collect() }
    }
    #[inline]
    fn get(&self, h: u64) -> Option<u16> {
        self.shards[(h >> 56) as usize % SHARDS].lock().unwrap().get(&h).copied()
    }
    fn put_all(&self, hs: &[u64], end: u16) {
        for &h in hs {
            let mut m = self.shards[(h >> 56) as usize % SHARDS].lock().unwrap();
            if m.len() >= SHARD_MAX {
                m.clear();
            }
            m.insert(h, end);
        }
    }
}

impl Default for EndCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Guided rollout: play the guide game's move whenever the same block exists, the
/// heuristic's choice otherwise, until the game ends. From a position on the guide game this
/// replays the rest of it; from a position that deviated it tests whether the deviation can
/// rejoin the guide's schedule and what that does to the ending. Returns (moves played,
/// whether the ending came from `cache`; the board is then not the end position).
fn play_out_guided(b: &mut Board, guide: &[[u8; 4]], scan: usize, hp: &HParams, record: Option<&mut Vec<u8>>, cache: Option<&EndCache>) -> (usize, bool) {
    let start = b.moves;
    let mut rec = record;
    let mut seen: Vec<u64> = Vec::new();
    let (end, hit) = loop {
        if let Some(c) = cache {
            let h = b.hash64();
            if let Some(end) = c.get(h) {
                break (end, true);
            }
            seen.push(h);
        }
        let m = match guided_move(b, guide, scan) {
            Some(m) => m,
            None => match choose_hp(b, Policy::Greedy, hp) {
                Some(m) => m,
                None => break (b.moves, false),
            },
        };
        b.play(m).unwrap();
        if let Some(r) = rec.as_mut() {
            r.push(m as u8);
        }
    };
    if let Some(c) = cache {
        c.put_all(&seen, end);
    }
    ((end - start) as usize, hit)
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
            hp: HParams::default(),
            wchain: 0.0,
            wsync: 0.0,
            sync_pairs: 0b101,
            nocluster: 0,
            gscan: 0,
            gtodo: false,
            gahead: 24,
            gcache: true,
            guided: false,
            guide: None,
        }
    }
}

pub fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

impl Config {
    /// Parse the search flags shared by `rbeam`, `ratchet` and `abtest`.
    /// `--hparams FILE|name=v,...` sets this configuration's heuristic factors.
    pub fn from_args(args: &[String]) -> Config {
        let d = Config::default();
        let (hparams, hp) = match arg::<String>(args, "--hparams") {
            Some(h) => {
                let (text, label) = match std::fs::read_to_string(&h) {
                    Ok(t) => (t, std::path::Path::new(&h).file_stem().unwrap().to_string_lossy().to_string()),
                    Err(_) => (h.clone(), "inline".to_string()),
                };
                (label, HParams::parse(&text))
            }
            None => (d.hparams.clone(), d.hp),
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
                // No heuristic rollout: for guided searches, where the guided rollout is the
                // evaluation (a greedy rollout still runs when there is no guide).
                "none" => vec![],
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
            hp,
            wchain: arg(args, "--wchain").unwrap_or(d.wchain),
            wsync: arg(args, "--wsync").unwrap_or(d.wsync),
            sync_pairs: arg(args, "--syncpairs").unwrap_or(d.sync_pairs),
            nocluster: arg(args, "--nocluster").unwrap_or(d.nocluster),
            gscan: arg(args, "--gscan").unwrap_or(d.gscan),
            gtodo: arg::<u8>(args, "--gtodo").unwrap_or(0) != 0,
            gahead: arg(args, "--gahead").unwrap_or(d.gahead),
            gcache: arg::<u8>(args, "--gcache").unwrap_or(1) != 0,
            guided: arg::<u8>(args, "--guide").unwrap_or(0) != 0,
            guide: None,
        }
    }

    pub fn policy_name(&self) -> &'static str {
        match self.pols.as_slice() {
            [Policy::Greedy] => "greedy",
            [Policy::AvoidBig] => "avoidbig",
            [] => "none",
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
        if self.wchain != 0.0 {
            t.push_str(&format!("_wc{}", self.wchain));
        }
        if self.guide.is_some() {
            t.push_str(&format!("_g{}", self.gscan));
        }
        if self.nocluster > 0 {
            t.push_str(&format!("_nc{}", self.nocluster));
        }
        if self.wsync != 0.0 {
            t.push_str(&format!("_sy{}", self.wsync));
        }
        t.push_str(&format!("_s{}", self.seed));
        t
    }

    /// All settings, for RESULT lines.
    pub fn describe(&self) -> String {
        format!(
            "width={} k={} rollout={} tie={} policy={} stoch={} temp={} wtwin={} wstuck={} stuckt={} wdebt={} maxkids={} rankpen={} hparams={} wchain={} guide={} nocluster={} wsync={} seed={} noise={}",
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
            self.wchain,
            self.guide.is_some() as u8,
            self.nocluster,
            self.wsync,
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
    Guided,
}

/// Rollout with each deterministic policy plus `stoch` seeded stochastic play-outs; the
/// longest one counts. Returns (moves played, game over?, final board, kind, value), where
/// value is the best over the rollouts of length + wchain * capacity chain at the end.
///
/// `cache`, `best_len`: ending cache for the guided rollout and the longest complete game so
/// far; a cached ending that would beat it is replayed, so every new best is exact.
fn rollout(start: &Board, cfg: &Config, seed0: u64, cache: Option<&EndCache>, best_len: u16, st: Todo) -> (usize, bool, Board, Kind, f32) {
    let mut best: Option<(usize, bool, Board, Kind)> = None;
    let mut value = f32::NEG_INFINITY;
    let mut consider = |n: usize, over: bool, b: Board, kind: Kind| {
        let chain = cfg.wchain != 0.0 && !matches!(kind, Kind::Guided);
        let v = if chain { n as f32 + cfg.wchain * passive_bound(&b, 400).0 as f32 } else { n as f32 };
        if v > value {
            value = v;
        }
        if best.as_ref().is_none_or(|x| n > x.0) {
            best = Some((n, over, b, kind));
        }
    };
    for &pol in &cfg.pols {
        let mut b = start.clone();
        let (n, over) = play_out_hp(&mut b, pol, &cfg.hp, cfg.cap);
        consider(n, over, b, Kind::Det(pol));
    }
    for j in 0..cfg.stoch {
        let s = seed0.wrapping_add(j as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ 0xD1B54A32D192ED03;
        let mut b = start.clone();
        let (n, over) = play_out_stochastic_hp(&mut b, cfg.temp, s, cfg.cap, &cfg.hp);
        consider(n, over, b, Kind::Stoch(s));
    }
    if let Some(g) = &cfg.guide {
        let mut b = start.clone();
        let go = |b: &mut Board, cache: Option<&EndCache>| {
            if cfg.gtodo { play_out_todo(b, g, st, cfg.gahead, &cfg.hp, None, cache) } else { play_out_guided(b, g, cfg.gscan, &cfg.hp, None, cache) }
        };
        let (mut n, hit) = go(&mut b, cache);
        if hit && start.moves as usize + n > best_len as usize {
            b = start.clone();
            n = go(&mut b, None).0;
        }
        consider(n, true, b, Kind::Guided);
    }
    if cfg.pols.is_empty() && cfg.stoch == 0 && cfg.guide.is_none() {
        let mut b = start.clone();
        let (n, over) = play_out_hp(&mut b, Policy::Greedy, &cfg.hp, cfg.cap);
        consider(n, over, b, Kind::Det(Policy::Greedy));
    }
    let (n, over, b, kind) = best.unwrap();
    (n, over, b, kind, value)
}

fn replay_kind(b: &mut Board, kind: Kind, cfg: &Config, st: Todo) -> Vec<u8> {
    let (temp, hp) = (cfg.temp, &cfg.hp);
    let mut out = Vec::new();
    match kind {
        Kind::Guided => {
            let g = cfg.guide.as_ref().expect("guided rollout without a guide");
            if cfg.gtodo {
                play_out_todo(b, g, st, cfg.gahead, hp, Some(&mut out), None);
            } else {
                play_out_guided(b, g, cfg.gscan, hp, Some(&mut out), None);
            }
        }
        Kind::Det(pol) => {
            while let Some(m) = choose_hp(b, pol, hp) {
                b.play(m).unwrap();
                out.push(m as u8);
            }
        }
        Kind::Stoch(seed) => {
            let mut rng = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
            while let Some(m) = choose_stochastic_hp(b, temp, &mut rng, hp) {
                b.play(m).unwrap();
                out.push(m as u8);
            }
        }
    }
    out
}

/// The top-k heuristic moves, best first (k = 0: all legal moves).
pub fn top_k_moves(b: &Board, k: usize) -> Vec<u8> {
    top_k_moves_hp(b, k, &crate::heuristic::global())
}

/// `top_k_moves` with explicit heuristic factors.
pub fn top_k_moves_hp(b: &Board, k: usize, hp: &HParams) -> Vec<u8> {
    top_k_moves_opt(b, k, hp, false)
}

/// `avoid_clusters`: drop the moves that create a cluster, unless every move does.
pub fn top_k_moves_opt(b: &Board, k: usize, hp: &HParams, avoid_clusters: bool) -> Vec<u8> {
    let a = analyse(b);
    let ws = weights_with(b, &a, hp);
    let mut idx: Vec<(usize, f64)> = ws.iter().copied().enumerate().filter(|&(_, w)| w > 0.0).collect();
    if avoid_clusters && idx.iter().any(|&(i, _)| a.cluster[i] == 0) {
        idx.retain(|&(i, _)| a.cluster[i] == 0);
    }
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
    // To-do follower state per frontier node (placeholders unless cfg.gtodo with a guide).
    let use_todo = cfg.gtodo && cfg.guide.is_some();
    let mut ftodo: Vec<Todo> = vec![Todo::at(if use_todo { prefix.len() } else { 0 })];
    // Best complete game: (splits, level of the node the rollout started from, node index,
    // kind). Level L node = position after L moves past the start; its rollout is replayed
    // at the end (rollouts are deterministic). If the start is already over, the prefix is
    // the answer.
    let mut best: (u16, usize, u32, Kind) = (start.moves, 0, 0, Kind::Det(Policy::Greedy));
    let mut rollouts: u64 = 0;
    let seed = cfg.seed;
    let cache = (cfg.guide.is_some() && cfg.gcache).then(EndCache::new);

    loop {
        let level = history.len();
        // Generate children (parent, move, rank, hash, board), dedup, evaluate the distinct ones.
        let ftodo_ref = &ftodo;
        let mut children: Vec<(u32, u8, u8, u64, Board, Todo)> = frontier
            .par_iter()
            .enumerate()
            .flat_map_iter(|(pi, node)| {
                let mv = top_k_moves_opt(node, cfg.k, &cfg.hp, (node.moves as usize) < cfg.nocluster);
                mv.into_iter().enumerate().map(move |(r, m)| {
                    let mut st = ftodo_ref[pi];
                    if use_todo {
                        let bl = node.blocks[m as usize];
                        st.consume([bl.x, bl.y, bl.w, bl.h], node.moves as usize, cfg.guide.as_ref().unwrap(), cfg.gahead);
                    }
                    let mut c = node.clone();
                    c.play(m as usize);
                    let h = c.hash64() ^ if use_todo { st.key() } else { 0 };
                    (pi as u32, m, r as u8, h, c, st)
                })
            })
            .collect();
        if children.is_empty() {
            break;
        }
        // Same sort as session 2's rbeam, so earlier seeds reproduce their games exactly.
        children.par_sort_unstable_by_key(|c| c.3);
        children.dedup_by_key(|c| c.3);
        let best_len = best.0;
        let scored: Vec<(u32, u8, f32, u16, bool, Board, Kind, Todo)> = children
            .into_par_iter()
            .map(|(pi, m, r, _, c, st)| {
                if c.is_over() {
                    return (pi, m, f32::NEG_INFINITY, c.moves, true, c, Kind::Det(Policy::Greedy), st);
                }
                let seed0 = seed ^ (level as u64) << 40 ^ (pi as u64) << 8 ^ m as u64;
                let (n, over, end, kind, value) = rollout(&c, cfg, seed0, cache.as_ref(), best_len, st);
                let reached = c.moves + n as u16;
                let mut s = value;
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
                if cfg.wsync != 0.0 {
                    s -= cfg.wsync * crate::pipes::unison_dist(&c, cfg.sync_pairs);
                }
                if cfg.noise > 0.0 {
                    let z = mix(seed ^ (level as u64) << 40 ^ (pi as u64) << 8 ^ m as u64);
                    s += cfg.noise * (z >> 40) as f32 / (1u64 << 24) as f32;
                }
                (pi, m, s, reached, over, c, kind, st)
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
        let mut live: Vec<(u32, u8, f32, u16, Board, Todo)> = Vec::with_capacity(scored.len());
        for (j, (pi, m, s, reached, _over, c, _, st)) in scored.into_iter().enumerate() {
            if Some(j) == forced {
                live.push((pi, m, f32::INFINITY, reached, c, st));
            } else if !c.is_over() {
                live.push((pi, m, s, reached, c, st));
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
            for &(pi, m, s, reached, _, _) in &live {
                line.push_str(&format!(" {pi},{m},{:.1},{}", s, reached as i64 - depth as i64));
            }
            line.push('\n');
            w.write_all(line.as_bytes()).unwrap();
        }
        history.push(live.iter().map(|&(pi, m, _, _, _, _)| (pi, m)).collect());
        (frontier, ftodo) = live.into_iter().map(|(_, _, _, _, b, st)| (b, st)).unzip();

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
    let mut st = Todo::at(if use_todo { prefix.len() } else { 0 });
    for (i, &m) in moves.iter().enumerate() {
        if use_todo && i >= prefix.len() {
            let bl = b.blocks[m as usize];
            st.consume([bl.x, bl.y, bl.w, bl.h], i, cfg.guide.as_ref().unwrap(), cfg.gahead);
        }
        b.play(m as usize).expect("illegal move in reconstructed path");
    }
    moves.extend(replay_kind(&mut b, bkind, cfg, st));
    assert!(b.is_over(), "reconstructed game not over");
    assert_eq!(b.moves, splits, "reconstructed game length mismatch");
    Outcome { moves, splits, rollouts, secs: t0.elapsed().as_secs_f64() }
}

/// Polish a finished game: guided-only beam searches from move `from`, each guided by the
/// current game, repeated until a round finds nothing longer (at most `rounds` rounds). The
/// guided beam is a local search: it tries deviations from the game along its whole tail and
/// keeps the ones whose guided completion is longest, so it never returns a shorter game.
/// `width`, `gscan` and the heuristic factors come from `cfg`; `seed` varies the tie-breaks.
pub fn polish(moves: &[u8], from: usize, cfg: &Config, rounds: usize, seed: u64) -> Outcome {
    let t0 = Instant::now();
    let mut cur = moves.to_vec();
    let mut rollouts = 0u64;
    for r in 0..rounds {
        let pc = Config {
            pols: vec![],
            stoch: 0,
            guide: Some(guide_of(&cur)),
            seed: seed.wrapping_add(r as u64),
            noise: 0.5,
            wchain: 0.0,
            nocluster: 0,
            ..cfg.clone()
        };
        let o = run(&cur[..from.min(cur.len())], &pc, None, false);
        rollouts += o.rollouts;
        if o.moves.len() <= cur.len() {
            break;
        }
        cur = o.moves;
    }
    Outcome { splits: cur.len() as u16, moves: cur, rollouts, secs: t0.elapsed().as_secs_f64() }
}

/// Kick a finished game: keep its first `t` moves, play a different move there (one of the
/// heuristic's top `k` other than the game's own, chosen by `pick`), and complete the game
/// with a guided rollout that follows the original wherever it still can. Returns the new
/// complete game, or None if the position has no alternative. Most kicks end much sooner
/// than the original; `polish` from move t + 1 then searches for a repair.
pub fn kick(moves: &[u8], t: usize, pick: u64, k: usize, cfg: &Config) -> Option<Vec<u8>> {
    let mut b = Board::new();
    for &m in &moves[..t] {
        b.play(m as usize).expect("illegal move");
    }
    let alts: Vec<u8> = top_k_moves_hp(&b, k, &cfg.hp).into_iter().filter(|&m| m != moves[t]).collect();
    if alts.is_empty() {
        return None;
    }
    let alt = alts[(mix(pick) % alts.len() as u64) as usize];
    let mut out = moves[..t].to_vec();
    out.push(alt);
    let bl = b.blocks[alt as usize];
    b.play(alt as usize).unwrap();
    let guide = guide_of(moves);
    if cfg.gtodo {
        let mut st = Todo::at(t);
        st.consume([bl.x, bl.y, bl.w, bl.h], t, &guide, cfg.gahead);
        play_out_todo(&mut b, &guide, st, cfg.gahead, &cfg.hp, Some(&mut out), None);
    } else {
        play_out_guided(&mut b, &guide, cfg.gscan, &cfg.hp, Some(&mut out), None);
    }
    Some(out)
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
