//! Diagnostic: along a greedy game, how often does the greedy move create a big cluster
//! while a non-big alternative exists, and what does AvoidBig choose there?
use splt::engine::Board;
use splt::heuristic::{analyse, best_move, choose, makes_big_cluster, weights_from, Policy};

fn main() {
    let mut b = Board::new();
    let (mut big_moves, mut with_alt, mut differ) = (0, 0, 0);
    loop {
        let a = analyse(&b);
        let ws = weights_from(&b, &a);
        let Some(g) = best_move(&ws) else { break };
        let ab = choose(&b, Policy::AvoidBig).unwrap();
        if makes_big_cluster(&b, g, a.cluster[g]) {
            big_moves += 1;
            let alts = (0..b.len as usize).filter(|&i| ws[i] > 0.0 && !makes_big_cluster(&b, i, a.cluster[i])).count();
            if alts > 0 {
                with_alt += 1;
            }
            if big_moves <= 5 {
                let bl = b.blocks[g];
                println!("move {}: greedy splits block {g} ({}x{}) mask bits {} alts {alts} avoidbig -> {ab}", b.moves + 1, bl.w, bl.h, a.cluster[g].count_ones());
            }
        }
        if ab != g {
            differ += 1;
        }
        b.play(g).unwrap();
    }
    println!("greedy game {} splits: big-cluster moves {big_moves}, with non-big alternative {with_alt}, avoidbig differs at {differ}", b.moves);
    let mut b2 = Board::new();
    let (n, _) = splt::heuristic::play_out(&mut b2, Policy::AvoidBig, 0);
    println!("avoidbig game: {n} splits");
}
