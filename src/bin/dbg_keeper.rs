//! Debug: replay a keeper game and report, per move, the keeper's view of it.
//! usage: dbg_keeper GAME [--startboard FILE] [--from A] [--to B]
use splt::engine::{parse_moves, Board};
use splt::keeper::{score, unclean_after, view};
use splt::rollout_beam::arg;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    splt::rollout_beam::load_start(&args);
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("game"));
    let from: usize = arg(&args, "--from").unwrap_or(0);
    let to: usize = arg(&args, "--to").unwrap_or(usize::MAX);
    let mut b = Board::start();
    for &m in &moves {
        let mut legal = Vec::new();
        b.legal_moves(&mut legal);
        let mut nclean = 0;
        for &l in &legal {
            let mut t = b.clone();
            let info = t.play(l as usize).unwrap();
            if !(info.new_point_blocks > 0 && info.exploded == 0 && unclean_after(&t)) {
                nclean += 1;
            }
        }
        let info = b.play(m).expect("illegal");
        let n = b.moves as usize;
        if n >= from && n <= to {
            let s = score(&b);
            let v: Vec<String> = (0..4).map(|p| { let v = view(&b, p); format!("{}{}{}w{}d{}", v.events, if v.fresh { "f" } else { "" }, if v.due { "D" } else { "" }, v.work, v.deadline) }).collect();
            println!(
                "{n:>5} legal {:>2} clean {:>2} | made {} unclean {} | phi {} {:?} over {} cap {} doomed {} | {}",
                legal.len(),
                nclean,
                info.new_point_blocks,
                info.new_point_blocks > 0 && info.exploded == 0 && unclean_after(&b),
                s.phi,
                s.per_pipe,
                s.over,
                s.cap,
                s.doomed,
                v.join(" ")
            );
        }
    }
}
