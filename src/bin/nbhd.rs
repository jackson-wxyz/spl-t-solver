//! Single-deviation neighbourhood of a game under different guided followers.
//!
//! usage: nbhd <seq-file> [--back N] [--k K]
//!
//! For every position in the last N moves and every alternative among the heuristic's top K
//! moves there, play the alternative and complete the game with a guided rollout that tries
//! to get back onto the game's own schedule. Reports, per follower, how the completions
//! compare with the game's length: how many tie, how many are longer, and the best.
//! This measures how good a local-search neighbourhood each follower gives, without running
//! any beam.
use rayon::prelude::*;
use splt::engine::{parse_moves, Board};
use splt::heuristic::HParams;
use splt::rollout_beam::{arg, guide_of, play_out_aligned, play_out_todo, top_k_moves_hp, Todo};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves: Vec<u8> = parse_moves(&std::fs::read_to_string(&args[1]).expect("read sequence")).iter().map(|&m| m as u8).collect();
    let back: usize = arg(&args, "--back").unwrap_or(400);
    let k: usize = arg(&args, "--k").unwrap_or(4);
    let guide = guide_of(&moves);
    let hp = HParams::default();
    let total = moves.len();
    let from = total.saturating_sub(back);
    let mut b = Board::new();
    let mut nodes: Vec<(usize, Board, u8)> = Vec::new(); // (t, board before move t, alternative)
    for (t, &m) in moves.iter().enumerate() {
        if t >= from {
            for alt in top_k_moves_hp(&b, k, &hp) {
                if alt != m {
                    nodes.push((t, b.clone(), alt));
                }
            }
        }
        b.play(m as usize).unwrap();
    }
    let names = ["aligned", "aligned+scan24", "todo ahead 12", "todo ahead 24", "todo ahead 60"];
    let res: Vec<[usize; 5]> = nodes
        .par_iter()
        .map(|(t, pb, alt)| {
            let mut out = [0usize; 5];
            let bl = pb.blocks[*alt as usize];
            let g = [bl.x, bl.y, bl.w, bl.h];
            let mut c = pb.clone();
            c.play(*alt as usize).unwrap();
            for (j, o) in out.iter_mut().enumerate() {
                let mut x = c.clone();
                let n = match j {
                    0 => play_out_aligned(&mut x, &guide, 0, &hp),
                    1 => play_out_aligned(&mut x, &guide, 24, &hp),
                    _ => {
                        let ahead = [12, 24, 60][j - 2];
                        let mut st = Todo::at(*t);
                        st.consume(g, *t, &guide, ahead);
                        play_out_todo(&mut x, &guide, st, ahead, &hp, None, None).0
                    }
                };
                *o = t + 1 + n;
            }
            out
        })
        .collect();
    println!("{} splits; {} single deviations in the last {} moves", total, nodes.len(), back);
    println!("{:<16} {:>6} {:>6} {:>6} {:>7} {:>6}   distribution of (completion - game length)", "follower", "longer", "equal", "-1..-9", "-10..", "best");
    for (j, name) in names.iter().enumerate() {
        let d: Vec<i64> = res.iter().map(|r| r[j] as i64 - total as i64).collect();
        let longer = d.iter().filter(|&&x| x > 0).count();
        let equal = d.iter().filter(|&&x| x == 0).count();
        let near = d.iter().filter(|&&x| (-9..0).contains(&x)).count();
        let far = d.iter().filter(|&&x| x < -9).count();
        let mut sorted = d.clone();
        sorted.sort();
        let q = |f: f64| sorted[((sorted.len() - 1) as f64 * f) as usize];
        println!(
            "{:<16} {:>6} {:>6} {:>6} {:>7} {:>+6}   p10 {} p25 {} median {} p75 {} p90 {}",
            name,
            longer,
            equal,
            near,
            far,
            sorted[sorted.len() - 1],
            q(0.1),
            q(0.25),
            q(0.5),
            q(0.75),
            q(0.9)
        );
        let mut best: Vec<(i64, usize)> = d.iter().enumerate().filter(|&(_, &x)| x > 0).map(|(i, &x)| (x, nodes[i].0)).collect();
        best.sort_by(|a, b| b.cmp(a));
        best.truncate(8);
        if !best.is_empty() {
            println!("    gains (gain @ move): {}", best.iter().map(|(g, t)| format!("+{g} @ {t}")).collect::<Vec<_>>().join(", "));
        }
    }
}
