//! Print Josh Holder's heuristic weights along a sequence, in the same format as the
//! Python reference (`scratch: dump_weights.py`), for diffing the port against the original.
//! usage: josh_dump <seq-file> <from> <to>
use splt::engine::{parse_moves, Board};
use splt::heuristic::weights;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("read sequence"));
    let lo: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
    let hi: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(moves.len());
    let mut b = Board::new();
    let mut out = String::new();
    for (k, &m) in moves.iter().take(hi).enumerate() {
        b.play(m).expect("illegal move in sequence");
        if k + 1 >= lo {
            let ws = weights(&b);
            out.push_str(&format!("{}", k + 1));
            for (i, w) in ws.iter().enumerate() {
                if *w != 0.0 {
                    out.push_str(&format!(" {i}:{w:.4}"));
                }
            }
            out.push('\n');
        }
    }
    print!("{out}");
}
