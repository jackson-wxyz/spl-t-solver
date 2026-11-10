//! Which refill situations does a game rely on? For every move that refilled the top of the
//! board, record the set of blocks the refill created (as "WxH" shapes) and count how often
//! each combination occurs, with the first move where it happened.
//!
//! usage: refills <seq-file>...
//!
//! The refill rules are the reference emulator's, and its authors derived them from the real
//! game. This shows which combinations a solver game uses that the published record games
//! (which were played on the real game) never exercised.
use splt::engine::{parse_moves, Board};
use std::collections::BTreeMap;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let moves = parse_moves(&text);
        let mut b = Board::new();
        let mut hist: BTreeMap<String, (u32, usize)> = BTreeMap::new();
        for (k, &m) in moves.iter().enumerate() {
            let before: Vec<_> = b.blocks().to_vec();
            let info = b.play(m).expect("illegal move");
            if info.refill_cap == 0 && info.exploded == 0 {
                continue;
            }
            // Refill blocks are appended after the survivors; survivors keep their order, so
            // the new blocks are the trailing free blocks that sit in the top rows.
            let survivors = before.len() + 1 - info.exploded as usize;
            let mut shapes: Vec<String> = b.blocks()[survivors.min(b.len as usize)..].iter().filter(|x| x.pts == 0).map(|x| format!("{}x{}", x.w, x.h)).collect();
            if shapes.is_empty() {
                continue;
            }
            shapes.sort();
            let key = format!("{} cluster blocks exploded -> {}", info.exploded, shapes.join(" + "));
            let e = hist.entry(key).or_insert((0, k + 1));
            e.0 += 1;
        }
        println!("{path}: {} moves", moves.len());
        let mut rows: Vec<_> = hist.into_iter().collect();
        rows.sort_by(|a, b| b.1.0.cmp(&a.1.0));
        for (k, (n, first)) in rows {
            println!("  {n:>5}x  {k}   (first at move {first})");
        }
    }
}
