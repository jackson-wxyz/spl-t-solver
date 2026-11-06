//! Replay a move-index sequence and check it is legal to the end.
//!
//! usage: replay <sequence-file> [--show N]   (prints the board after move N; 0 = start)
//!        replay <sequence-file> --final       (prints the final board)
use splt::engine::{parse_moves, Board};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: replay <sequence-file> [--show N | --final]");
    let text = std::fs::read_to_string(path).expect("read sequence");
    let moves = parse_moves(&text);
    let show: Option<usize> = args.iter().position(|a| a == "--show").map(|i| args[i + 1].parse().unwrap());
    let show_final = args.iter().any(|a| a == "--final");

    let mut b = Board::new();
    if show == Some(0) {
        print!("{}", b.render());
    }
    for (k, &m) in moves.iter().enumerate() {
        if b.play(m).is_none() {
            println!("{path}: ILLEGAL move index {m} at split #{}\n{}", k + 1, b.render());
            std::process::exit(1);
        }
        if show == Some(k + 1) {
            print!("{}", b.render());
        }
        if let Some(msg) = b.check_invariants() {
            println!("{path}: invariant broken after split #{}: {msg}\n{}", k + 1, b.render());
            std::process::exit(2);
        }
    }
    if show_final {
        print!("{}", b.render());
    }
    let left = b.num_legal_moves();
    println!(
        "{path}: {} splits replayed legally; legal moves remaining = {left}{}",
        moves.len(),
        if left == 0 { " (game over - matches)" } else { " (game NOT over)" }
    );
}
