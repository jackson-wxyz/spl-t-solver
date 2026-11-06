//! Print evaluation features along a move sequence.  usage: trace <seq-file> [every=50]
use splt::engine::{parse_moves, Board};
use splt::search::{features, Params};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).unwrap());
    let every: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(50);
    let p = Params { w_model: 1.0, ..Params::default() };
    let mut b = Board::new();
    println!("{:>6} {:>8} {:>8} {:>6} {:>5} {:>7} {:>6} {:>6} {:>6} {:>6} {:>7}", "split", "survive", "disc", "cap", "imb", "debt", "ptarea", "legal", "slot", "death", "backlog");
    for (k, &m) in moves.iter().enumerate() {
        b.play(m).unwrap();
        if (k + 1) % every == 0 || k + 1 == moves.len() {
            let f = features(&b, &p);
            let pa: u32 = b.blocks().iter().filter(|x| x.pts > 0).map(|x| x.area()).sum();
            println!(
                "{:>6} {:>8.0} {:>8.1} {:>6.0} {:>5.0} {:>7.1} {:>6} {:>6} {:>6.0} {:>6.0} {:>7.0}",
                k + 1,
                f.survive,
                f.disc,
                f.k0,
                f.imbalance,
                f.debt,
                pa,
                b.num_legal_moves(),
                f.slot,
                f.death,
                f.backlog
            );
        }
    }
}
