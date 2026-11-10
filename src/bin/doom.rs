//! How early is the end of a game visible? For positions along a game, compare the moves
//! actually remaining with a capacity bound from the passive schedule.
//!
//! usage: doom <seq-file> [--last N] [--every K]
//!
//! The bound (`passive_bound`): every split uses one unit of capacity (sum of area - 1 over
//! free blocks), and capacity only comes back when point blocks explode and the hole refills.
//! Fast-forward the board with no splits and stop at the first explosion that the capacity
//! in hand cannot reach. New clusters are worth the split count, so late in a game they
//! cannot explode for many dozens of moves and the bound is close to a true upper bound on
//! the moves left; early in a game new clusters do explode within the horizon and it is not.
//! Prints: move, free capacity, legal moves, bound, moves actually left, greedy rollout.
use splt::engine::{parse_moves, Board};
use splt::heuristic::{play_out, Policy};
use splt::rollout_beam::arg;
use splt::search::{capacity, passive_bound};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("read sequence"));
    let last: usize = arg(&args, "--last").unwrap_or(400);
    let every: usize = arg(&args, "--every").unwrap_or(5);
    let total = moves.len();
    let mut b = Board::new();
    println!("{:>5} {:>4} {:>5} {:>6} {:>6} {:>6} {:>6}", "move", "cap", "legal", "bound", "left", "slack", "greedy");
    let (mut first_tight, mut last_loose) = (None, 0usize);
    for k in 0..=total {
        if k + last >= total {
            let (bound, _) = passive_bound(&b, 1000);
            let left = total - k;
            if bound as usize > left {
                last_loose = k;
            } else if first_tight.is_none() {
                first_tight = Some(k);
            }
            if (total - k) % every == 0 {
                let mut t = b.clone();
                let (g, _) = play_out(&mut t, Policy::Greedy, 0);
                println!("{:>5} {:>4} {:>5} {:>6} {:>6} {:>6} {:>6}", k, capacity(&b).0, b.num_legal_moves(), bound, left, bound as i64 - left as i64, g);
            }
        }
        if k < total {
            b.play(moves[k]).expect("illegal move");
        }
    }
    println!("first position (in the window) where the bound equals the moves left: {:?}; last position where it exceeds them: {}", first_tight, last_loose);
}
