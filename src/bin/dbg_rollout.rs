//! Diagnostic: greedy / avoid-big rollouts from positions along a sequence.
//! usage: dbg_rollout <seq> <keep> [<keep> ...]   prints rollout lengths and the final board
use splt::engine::{parse_moves, Board};
use splt::heuristic::{play_out, Policy};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("read"));
    for keep in args[2..].iter().map(|s| s.parse::<usize>().unwrap()) {
        let mut b = Board::new();
        for &m in &moves[..keep] {
            b.play(m).unwrap();
        }
        let mut g = b.clone();
        let (ng, _) = play_out(&mut g, Policy::Greedy, 0);
        let mut a = b.clone();
        let (na, _) = play_out(&mut a, Policy::AvoidBig, 0);
        println!(
            "from move {keep}: greedy +{ng} -> {}, avoidbig +{na} -> {} (sequence itself reaches {}); twins here {} / at greedy end {}",
            keep + ng,
            keep + na,
            moves.len(),
            splt::pipes::twins(&b, 1.5, 20),
            splt::pipes::twins(&g, 1.5, 20)
        );
        print!("{}", g.render());
    }
}
