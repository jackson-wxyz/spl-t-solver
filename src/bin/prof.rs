//! Micro-profile of the operations the beam search spends its time on, measured on every
//! child of every position along a real game.  usage: prof <seq-file>
use splt::engine::{parse_moves, Board};
use splt::search::{features, Params};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).unwrap());
    let p = Params::default();

    // Collect (parent, move) pairs for every legal move along the game.
    let mut parents = Vec::new();
    let mut b = Board::new();
    for &m in &moves {
        parents.push(b.clone());
        b.play(m).unwrap();
    }
    let mut pairs = Vec::new();
    let mut mv = Vec::new();
    for (i, pb) in parents.iter().enumerate() {
        pb.legal_moves(&mut mv);
        for &m in &mv {
            pairs.push((i, m));
        }
    }
    let n = pairs.len() as f64;
    println!("{} positions, {} children (avg branching {:.1})", parents.len(), pairs.len(), n / parents.len() as f64);

    let t = Instant::now();
    let mut kids: Vec<Board> = Vec::with_capacity(pairs.len());
    for &(i, _) in &pairs {
        kids.push(parents[i].clone());
    }
    let clone_ns = t.elapsed().as_nanos() as f64 / n;

    let t = Instant::now();
    for (k, &(_, m)) in kids.iter_mut().zip(&pairs) {
        black_box(k.play(m as usize));
    }
    let play_ns = t.elapsed().as_nanos() as f64 / n;

    let t = Instant::now();
    let mut c = 0;
    for k in &kids {
        c += k.is_over() as usize;
    }
    black_box(c);
    let over_ns = t.elapsed().as_nanos() as f64 / n;

    let t = Instant::now();
    let mut h = 0u64;
    for k in &kids {
        h ^= k.hash64();
    }
    black_box(h);
    let hash_ns = t.elapsed().as_nanos() as f64 / n;

    let live: Vec<&Board> = kids.iter().filter(|k| !k.is_over()).collect();
    let t = Instant::now();
    let mut s = 0.0;
    for k in &live {
        s += features(k, &p).survive;
    }
    black_box(s);
    let feat_ns = t.elapsed().as_nanos() as f64 / live.len() as f64;

    // Passive-sim internals: number of events and cost of one passive step.
    let mut events = 0usize;
    let t = Instant::now();
    for k in &live {
        let mut tb = (*k).clone();
        let mut el = 0u16;
        while let Some(dt) = tb.next_explosion() {
            if el + dt > p.horizon {
                break;
            }
            tb.passive_step(el);
            el += dt;
            events += 1;
        }
    }
    let passive_total = t.elapsed().as_nanos() as f64;
    let pm = Params { w_model: 1.0, ..p };
    let t = Instant::now();
    let mut s = 0.0;
    for k in &live {
        s += features(k, &pm).death;
    }
    black_box(s);
    let feat_model_ns = t.elapsed().as_nanos() as f64 / live.len() as f64;

    // Duplicate children (transpositions) among siblings of the same parent.
    let mut dup = 0usize;
    let mut start = 0;
    while start < pairs.len() {
        let pi = pairs[start].0;
        let mut end = start;
        while end < pairs.len() && pairs[end].0 == pi {
            end += 1;
        }
        let mut hs: Vec<u64> = kids[start..end].iter().map(|k| k.hash64()).collect();
        let before = hs.len();
        hs.sort_unstable();
        hs.dedup();
        dup += before - hs.len();
        start = end;
    }

    println!("per child: clone {clone_ns:.0} ns, play {play_ns:.0} ns, is_over {over_ns:.0} ns, hash {hash_ns:.0} ns");
    println!("per live child: features {feat_ns:.0} ns  (with pipeline model {feat_model_ns:.0} ns)");
    println!(
        "passive sim: {:.1} events/eval, {:.0} ns/event, {:.0} ns/eval",
        events as f64 / live.len() as f64,
        passive_total / events as f64,
        passive_total / live.len() as f64
    );
    #[cfg(feature = "prof")]
    {
        use splt::engine::phase;
        let tot: u64 = phase::TICKS.iter().map(|t| t.load(std::sync::atomic::Ordering::Relaxed)).sum();
        for (i, t) in phase::TICKS.iter().enumerate() {
            let v = t.load(std::sync::atomic::Ordering::Relaxed);
            if v > 0 {
                println!("  resolve phase {:<16} {:>5.1}%", phase::NAMES[i], 100.0 * v as f64 / tot as f64);
            }
        }
    }
    println!("sibling duplicates: {dup} of {} children ({:.1}%)", pairs.len(), 100.0 * dup as f64 / n);
}
