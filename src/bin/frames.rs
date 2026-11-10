//! Print the board after every move of a game, one line per position, for checking another
//! implementation of the rules against this engine (the blog's JavaScript player is checked
//! this way, see tools/blog/verify.mjs).
//!
//! usage: frames <seq-file>
//!
//! Line k (from 0) is the position after k moves: the blocks in engine order, each as
//! "x y w h pts", separated by ";".
use splt::engine::{parse_moves, Board};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("sequence file"));
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    let mut b = Board::new();
    let line = |b: &Board| b.blocks().iter().map(|x| format!("{} {} {} {} {}", x.x, x.y, x.w, x.h, x.pts)).collect::<Vec<_>>().join(";");
    writeln!(out, "{}", line(&b)).unwrap();
    for &m in &moves {
        b.play(m).expect("illegal move");
        writeln!(out, "{}", line(&b)).unwrap();
    }
}
