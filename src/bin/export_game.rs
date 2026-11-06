//! Export a move sequence as one board per move, for following it by hand on the real game
//! with `tools/replay.html`.
//!
//! usage: export_game <sequence-file> <out.json>
//!
//! Output is a JSON array with one frame per position, k = 0 (the start) .. N (the final
//! board), one frame per line:
//!   {"move":k,"next":"H"|"V","tap":{"x":..,"y":..,"w":..,"h":..}|null,"blocks":[{"x":..,"y":..,"w":..,"h":..,"pts":..},..]}
//! `blocks` is the board after k splits and `tap` the block to split next (null on the final
//! frame). x is the column 0..7 from the left, y the row 0..15 from the top; `pts` > 0 marks a
//! point block and holds its countdown, 0 is a free block. The sequence is replayed with the
//! engine and checked to be legal at every move.
use splt::engine::{parse_moves, Block, Board};
use std::fmt::Write as _;

fn rect(out: &mut String, b: &Block) {
    write!(out, "\"x\":{},\"y\":{},\"w\":{},\"h\":{}", b.x, b.y, b.w, b.h).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: export_game <sequence-file> <out.json>");
        std::process::exit(2);
    }
    let (seq_path, out_path) = (&args[1], &args[2]);
    let text = std::fs::read_to_string(seq_path).unwrap_or_else(|e| panic!("read {seq_path}: {e}"));
    let moves = parse_moves(&text);

    let mut b = Board::new();
    let mut out = String::with_capacity(4 << 20);
    out.push_str("[\n");
    for k in 0..=moves.len() {
        let tap = moves.get(k).copied();
        if let Some(m) = tap {
            if !b.can_split(m) {
                eprintln!("{seq_path}: ILLEGAL move index {m} at split #{}\n{}", k + 1, b.render());
                std::process::exit(1);
            }
        }
        write!(out, "{{\"move\":{},\"next\":\"{}\",\"tap\":", b.moves, if b.horizontal_next() { "H" } else { "V" }).unwrap();
        match tap {
            Some(m) => {
                out.push('{');
                rect(&mut out, &b.blocks[m]);
                out.push('}');
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"blocks\":[");
        for (i, blk) in b.blocks().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('{');
            rect(&mut out, blk);
            write!(out, ",\"pts\":{}}}", blk.pts).unwrap();
        }
        out.push_str("]}");
        out.push_str(if tap.is_some() { ",\n" } else { "\n" });

        if let Some(m) = tap {
            b.play(m).expect("move checked legal above");
            if let Some(msg) = b.check_invariants() {
                eprintln!("{seq_path}: invariant broken after split #{}: {msg}\n{}", k + 1, b.render());
                std::process::exit(1);
            }
        }
    }
    out.push_str("]\n");
    std::fs::write(out_path, &out).unwrap_or_else(|e| panic!("write {out_path}: {e}"));

    let left = b.num_legal_moves();
    println!(
        "{out_path}: {} splits, {} frames, {} KB{}",
        b.moves,
        moves.len() + 1,
        out.len() / 1024,
        if left == 0 { " (game over - matches)".to_string() } else { format!(" (game NOT over: {left} legal moves remain)") }
    );
}
