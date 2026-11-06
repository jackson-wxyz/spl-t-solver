//! Determinism / equivalence check. Prints a digest of engine states and evaluation features
//! for many positions. Build it normally and with `--features reference`, then diff the
//! outputs: they must be identical.
//!
//! usage: selfcheck [games] [record-seq-file ...]
use splt::engine::{parse_moves, Board};
use splt::search::{features, Params};

fn order_hash(b: &Board) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for bl in b.blocks() {
        for v in [bl.x as u64, bl.y as u64, bl.w as u64, bl.h as u64, bl.pts as u64] {
            h = (h ^ v).wrapping_mul(0x100000001b3);
        }
    }
    h ^ (b.moves as u64) << 48 ^ (b.next_explosion().unwrap_or(0) as u64) << 32
}

fn feat_digest(b: &Board) -> String {
    let p = Params { w_model: 1.0, alt: true, ..Params::default() };
    let f = features(b, &p);
    [f.k0, f.survive, f.disc, f.imbalance, f.debt, f.slot, f.death, f.backlog, f.stuck]
        .iter()
        .map(|x| format!("{:08x}", x.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let games: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(2000);
    let mut rng: u64 = 0x2545F4914F6CDD1D;
    let mut mv = Vec::new();
    for g in 0..games {
        let mut b = Board::new();
        let mut line = format!("game {g}:");
        loop {
            b.legal_moves(&mut mv);
            if mv.is_empty() {
                break;
            }
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let m = mv[(rng % mv.len() as u64) as usize];
            b.play(m as usize).unwrap();
            line.push_str(&format!(" {:x}", order_hash(&b)));
            if b.moves % 7 == 0 {
                line.push_str(&format!(" [{}]", feat_digest(&b)));
            }
        }
        println!("{line}");
    }
    // Late-game coverage: every child of every position along the given sequences.
    for path in &args[2..] {
        let moves = parse_moves(&std::fs::read_to_string(path).unwrap());
        let mut b = Board::new();
        for (k, &m) in moves.iter().enumerate() {
            b.legal_moves(&mut mv);
            let mut line = format!("{path} @{k}:");
            for &c in &mv {
                let mut child = b.clone();
                child.play(c as usize).unwrap();
                line.push_str(&format!(" {:x}", order_hash(&child)));
                if !child.is_over() {
                    line.push_str(&format!(" [{}]", feat_digest(&child)));
                }
            }
            println!("{line}");
            b.play(m).unwrap();
        }
    }
}
