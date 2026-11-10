//! Opening statistics of games, one CSV line per sequence file, to test whether the way a
//! game opens predicts how long it lasts.
//!
//! usage: opening <seq-file>...
//!
//! Columns: file, splits, first (move that created the first cluster), cl30 / cl60 (cluster
//! moves among the first 30 / 60), big100 (cluster moves in the first 100 whose blocks were
//! bigger than 1x1), lost100 (splitting capacity those clusters froze), pts60 (cells held by
//! point blocks after 60 moves), pipes3 (first move with at least three clusters in every
//! 2-column pipe), free200 (free cells after 200 moves).
use splt::engine::{parse_moves, Board};
use splt::pipes::clusters;

fn main() {
    println!("file,splits,first,cl30,cl60,big100,lost100,pts60,pipes3,free200");
    for path in std::env::args().skip(1) {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if !text.starts_with('[') {
            continue;
        }
        let moves = parse_moves(&text);
        let mut b = Board::new();
        let (mut first, mut cl30, mut cl60, mut big100, mut lost100, mut pts60, mut pipes3, mut free200) = (0usize, 0, 0, 0, 0u32, 0u32, 0usize, 0u32);
        let mut ok = true;
        for (k, &m) in moves.iter().enumerate() {
            let Some(info) = b.play(m) else {
                ok = false;
                break;
            };
            let t = k + 1;
            if info.new_point_blocks > 0 {
                if first == 0 {
                    first = t;
                }
                if t <= 30 {
                    cl30 += 1;
                }
                if t <= 60 {
                    cl60 += 1;
                }
                if t <= 100 {
                    lost100 += info.new_point_cap;
                    if info.new_point_cap > 0 {
                        big100 += 1;
                    }
                }
            }
            if t == 60 {
                pts60 = b.blocks().iter().filter(|x| x.pts > 0).map(|x| x.area()).sum();
            }
            if t == 200 {
                free200 = b.blocks().iter().filter(|x| x.pts == 0).map(|x| x.area()).sum();
            }
            if pipes3 == 0 && t % 2 == 0 && (0..4).all(|p| clusters(&b, p).len() >= 3) {
                pipes3 = t;
            }
        }
        if ok {
            println!("{path},{},{first},{cl30},{cl60},{big100},{lost100},{pts60},{pipes3},{free200}", moves.len());
        }
    }
}
