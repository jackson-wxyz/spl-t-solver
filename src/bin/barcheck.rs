//! What can be done with the blocks a refill delivers, when nothing else on the board can
//! be split? Exhaustive search with the real engine.
//!
//! The board is filled with point blocks that will not explode for a long time, except for
//! the blocks of one scenario at the top (a 2x2 square over one pipe, a 4x2 plank over a
//! pair, an 8x2 bar over all four, ...). From there every sequence of legal moves is tried,
//! for both directions of the first split. Reported: the longest sequence of moves that can
//! be made at all, and the numbers of moves after which every block of the scenario has
//! become part of a cluster (the only moments at which the next explosion may come without
//! leaving unfinished blocks behind).
//!
//! usage: barcheck
use splt::engine::{Block, Board, H, W};
use std::collections::{BTreeSet, HashSet};

fn build(free: &[(u8, u8, u8, u8)], moves: u16) -> Board {
    let mut covered = [false; W * H];
    let mut blocks = Vec::new();
    for &(x, y, w, h) in free {
        blocks.push(Block { x, y, w, h, pts: 0 });
        for yy in y..y + h {
            for xx in x..x + w {
                covered[yy as usize * W + xx as usize] = true;
            }
        }
    }
    for y in 0..H {
        for x in 0..W {
            if !covered[y * W + x] {
                blocks.push(Block { x: x as u8, y: y as u8, w: 1, h: 1, pts: 3000 });
            }
        }
    }
    Board::from_parts(&blocks, moves)
}

struct Search {
    seen: HashSet<u64>,
    longest: usize,
    clean: BTreeSet<usize>,
    best_line: Vec<String>,
    line: Vec<String>,
}

impl Search {
    fn dfs(&mut self, b: &Board, depth: usize) {
        if !self.seen.insert(b.hash64()) {
            return;
        }
        if depth > 0 && b.blocks().iter().all(|x| x.pts > 0) {
            self.clean.insert(depth);
        }
        if depth > self.longest {
            self.longest = depth;
            self.best_line = self.line.clone();
        }
        let mut legal = Vec::new();
        b.legal_moves(&mut legal);
        for &m in &legal {
            let bl = b.blocks[m as usize];
            let mut t = b.clone();
            let info = t.play(m as usize).unwrap();
            assert_eq!(info.exploded, 0);
            self.line.push(format!("{}{}x{}", if b.horizontal_next() { "H" } else { "V" }, bl.w, bl.h));
            self.dfs(&t, depth + 1);
            self.line.pop();
        }
    }
}

fn main() {
    let scenarios: Vec<(&str, Vec<(u8, u8, u8, u8)>)> = vec![
        ("one 2x2 square (a single pipe)", vec![(0, 0, 2, 2)]),
        ("one 4x2 plank (a pair in unison)", vec![(0, 0, 4, 2)]),
        ("two 2x2 squares at once", vec![(0, 0, 2, 2), (2, 0, 2, 2)]),
        ("two 4x2 planks at once", vec![(0, 0, 4, 2), (4, 0, 4, 2)]),
        ("a 4x2 plank with a 2x2 square on each side", vec![(0, 0, 2, 2), (2, 0, 4, 2), (6, 0, 2, 2)]),
        ("one 8x2 bar (all four pipes in unison)", vec![(0, 0, 8, 2)]),
        ("an 8x2 bar plus one leftover 1x2 block below it", vec![(0, 0, 8, 2), (0, 2, 1, 2)]),
        ("an 8x2 bar plus two leftover 1x2 blocks below it", vec![(0, 0, 8, 2), (0, 2, 1, 2), (1, 2, 1, 2)]),
        ("an 8x2 bar plus one leftover 2x1 block below it", vec![(0, 0, 8, 2), (0, 2, 2, 1)]),
    ];
    for (name, free) in &scenarios {
        let cap: u32 = free.iter().map(|f| f.2 as u32 * f.3 as u32 - 1).sum();
        println!("{name}: {cap} splits of capacity");
        // Split counts 2000 and 2001: the next split is horizontal (cuts top from bottom) or
        // vertical (cuts left from right).
        for (moves, first) in [(2000u16, "horizontal"), (2001u16, "vertical")] {
            let b = build(free, moves);
            assert!(b.check_invariants().is_none(), "{:?}", b.check_invariants());
            let mut s = Search { seen: HashSet::new(), longest: 0, clean: BTreeSet::new(), best_line: Vec::new(), line: Vec::new() };
            s.dfs(&b, 0);
            println!(
                "  first split {first:<10}: at most {:>2} moves; everything clustered after {:?} moves; {} positions; a longest line: {}",
                s.longest,
                s.clean,
                s.seen.len(),
                s.best_line.join(" ")
            );
        }
    }
}
