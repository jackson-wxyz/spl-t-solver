//! Per-move statistics of a game, for visualization and analysis.
//!
//! usage: stats <seq-file> [--every N] [--rollout] [--json OUT]
//!
//! For every move (or every N moves) prints: split count, direction, number of blocks,
//! free cells, point-block cells, legal moves, moves until the next explosion, whether the
//! move created a cluster (and of what block size), and per pipe (columns 0-1, 2-3, 4-5,
//! 6-7) the cluster values from bottom to top. With --rollout, also how many more moves a
//! greedy heuristic rollout survives from the position (the beam's evaluation signal).
//! --json writes the same as a JSON array of objects.
use splt::engine::{parse_moves, Board};
use splt::heuristic::{play_out, Policy};
use splt::pipes::clusters;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("read sequence"));
    let every: usize = arg(&args, "--every").unwrap_or(1);
    let do_roll = args.iter().any(|a| a == "--rollout");
    let json_path: Option<String> = arg(&args, "--json");
    // --alts: for every move, the greedy rollout length of each of the top-4 heuristic moves
    // and the rank of the move the game actually took.
    let do_alts = args.iter().any(|a| a == "--alts");

    let mut b = Board::new();
    let (mut refill_cap_sum, mut lost_cap_sum, mut refill_blocks, mut big_refills) = (0u64, 0u64, 0u64, 0u64);
    let mut rows: Vec<String> = Vec::new();
    let mut json: Vec<String> = Vec::new();
    let total = moves.len();
    println!("{:>5} {:>2} {:>4} {:>4} {:>4} {:>5} {:>4} {:>7} {:>5}  pipes (bottom -> top)", "move", "d", "blk", "free", "pts", "legal", "next", "cluster", "roll");
    for k in 0..=total {
        // Statistics of the position before move k+1 (k = number of moves made).
        if k % every == 0 || k == total {
            let free: u32 = b.blocks().iter().filter(|x| x.pts == 0).map(|x| x.area()).sum();
            let pts: u32 = b.blocks().iter().filter(|x| x.pts > 0).map(|x| x.area()).sum();
            let legal = b.num_legal_moves();
            let next = b.next_explosion().unwrap_or(0);
            let dir = if b.horizontal_next() { "H" } else { "V" };
            // Does the move about to be played create a cluster, and of which block size?
            let cluster = if k < total {
                let m = moves[k];
                let bl = b.blocks[m];
                let mut t = b.clone();
                let info = t.play(m).expect("illegal move");
                if info.new_point_blocks > 0 {
                    let (w, h) = if b.horizontal_next() { (bl.w, bl.h / 2) } else { (bl.w / 2, bl.h) };
                    format!("{}x{}x{}", info.new_point_blocks, w, h)
                } else {
                    "-".to_string()
                }
            } else {
                "end".to_string()
            };
            let roll = if do_roll && k < total {
                let mut t = b.clone();
                let (n, _) = play_out(&mut t, Policy::Greedy, 0);
                n as i64
            } else {
                -1
            };
            // Alternatives: top-4 heuristic moves, their rollout lengths, and the taken rank.
            let (alts, taken_rank) = if do_alts && k < total {
                let ws = splt::heuristic::weights(&b);
                let mut idx: Vec<(usize, f64)> = ws.iter().copied().enumerate().filter(|&(_, w)| w > 0.0).collect();
                idx.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                idx.truncate(4);
                let lens: Vec<i64> = idx
                    .iter()
                    .map(|&(i, _)| {
                        let mut t = b.clone();
                        t.play(i).unwrap();
                        let (n, _) = play_out(&mut t, Policy::Greedy, 0);
                        n as i64 + 1
                    })
                    .collect();
                let rank = idx.iter().position(|&(i, _)| i == moves[k]).map(|r| r as i64 + 1).unwrap_or(0);
                (lens, rank)
            } else {
                (Vec::new(), 0)
            };
            let pipes: Vec<Vec<u16>> = (0..4).map(|p| clusters(&b, p)).collect();
            let pipes_s: Vec<String> = pipes.iter().map(|v| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("/")).collect();
            rows.push(format!(
                "{:>5} {:>2} {:>4} {:>4} {:>4} {:>5} {:>4} {:>7} {:>5}  {}",
                k,
                dir,
                b.len,
                free,
                pts,
                legal,
                next,
                cluster,
                if roll >= 0 { roll.to_string() } else { "".to_string() },
                pipes_s.join(" | ")
            ));
            if json_path.is_some() {
                let pj: Vec<String> = pipes.iter().map(|v| format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))).collect();
                let aj: Vec<String> = alts.iter().map(|x| x.to_string()).collect();
                json.push(format!(
                    "{{\"move\":{k},\"dir\":\"{dir}\",\"blocks\":{},\"free\":{free},\"pts\":{pts},\"legal\":{legal},\"next\":{next},\"cluster\":\"{cluster}\",\"roll\":{roll},\"alts\":[{}],\"rank\":{taken_rank},\"pipes\":[{}]}}",
                    b.len,
                    aj.join(","),
                    pj.join(",")
                ));
            }
        }
        if k < total {
            let before = b.len as usize;
            let info = b.play(moves[k]).expect("illegal move");
            refill_cap_sum += info.refill_cap as u64;
            lost_cap_sum += info.new_point_cap as u64;
            // refill blocks are appended last; count those bigger than 2x2
            let after = b.len as usize;
            let _ = before;
            for bl in b.blocks()[..after].iter().rev().take(4) {
                if bl.pts == 0 && bl.area() > 4 && bl.y == 0 {
                    big_refills += 1;
                    break;
                }
            }
            if info.refill_cap > 0 {
                refill_blocks += 1;
            }
        }
    }
    eprintln!(
        "capacity accounting: moves {total} = 127 + refill capacity {refill_cap_sum} - capacity lost to clusters {lost_cap_sum} - leftover {}; moves with refills {refill_blocks}",
        127 + refill_cap_sum as i64 - lost_cap_sum as i64 - total as i64
    );
    for r in &rows {
        println!("{r}");
    }
    if let Some(p) = json_path {
        std::fs::write(&p, format!("[\n{}\n]\n", json.join(",\n"))).expect("write json");
        eprintln!("wrote {p}");
    }
}
