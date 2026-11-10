//! How each of the four pipes is doing along a game.
//!
//! usage: pipewatch <seq-file> [--from A] [--to B] [--every N] [--window W] [--render M] [--list 1]
//!                  [--startboard FILE]   (a game from a `whatif` position; the window table
//!                                         then counts moves from that position)
//!
//! Every N moves between A and B: the cluster values in each pipe (bottom to top), then the
//! free splitting capacity and the stuck point area per pipe. Then, per window of W moves:
//! how many explosions each pipe had and how much refill capacity came back in total.
//! --render M prints the board after move M.
use splt::engine::{parse_moves, Board, EMPTY, H, W};
use splt::pipes::{breaks_pipe, clusters};
use splt::rollout_beam::arg;

/// (free capacity, point-block area, void cells) of pipe `p`. A block wider than the pipe
/// counts by the share of its cells that lie in the pipe.
fn pipe_stats(b: &Board, p: usize) -> (f32, u32, u32) {
    let (mut cap, mut pa, mut void) = (0.0f32, 0u32, 0u32);
    for y in 0..H {
        for x in 2 * p..2 * p + 2 {
            let id = b.grid[y * W + x];
            if id == EMPTY {
                void += 1;
                continue;
            }
            let bl = &b.blocks[id as usize];
            if bl.pts > 0 {
                pa += 1;
            } else {
                cap += (bl.area() - 1) as f32 / bl.area() as f32;
            }
        }
    }
    (cap, pa, void)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves = parse_moves(&std::fs::read_to_string(&args[1]).expect("sequence file"));
    let from: usize = arg(&args, "--from").unwrap_or(0);
    let to: usize = arg(&args, "--to").unwrap_or(usize::MAX);
    let every: usize = arg(&args, "--every").unwrap_or(50);
    let window: usize = arg(&args, "--window").unwrap_or(100);
    let render: Option<usize> = arg(&args, "--render");
    // --list 1: one line per move between A and B (direction, block, what it caused).
    let list = arg::<u8>(&args, "--list").unwrap_or(0) != 0;
    splt::rollout_beam::load_start(&args);
    let mut b = Board::start();
    let base = b.moves as usize;
    let mut expl = vec![[0u32; 4]; moves.len() / window + 1];
    let mut refill = vec![0u32; moves.len() / window + 1];
    let mut made = vec![[0u32; 4]; moves.len() / window + 1];
    // Moves that break a pipe (see `pipes::breaks_pipe`): plugs and burials per pipe.
    let mut plugs = vec![[0u32; 4]; moves.len() / window + 1];
    let mut buries = vec![[0u32; 4]; moves.len() / window + 1];
    for (k, &m) in moves.iter().enumerate() {
        let bl = b.blocks[m];
        let wi = k / window;
        let br = breaks_pipe(&b, m);
        if br & 1 != 0 {
            plugs[wi][(bl.x / 2) as usize] += 1;
        }
        if br & 2 != 0 {
            buries[wi][(bl.x / 2) as usize] += 1;
        }
        let horiz = b.horizontal_next();
        let info = b.play(m).expect("illegal move");
        if list && base + k + 1 >= from && base + k + 1 <= to {
            println!(
                "{:>5} {} pipe {}{} {}x{} at ({},{}){}{}{}",
                base + k + 1,
                if horiz { "H" } else { "V" },
                bl.x / 2,
                if bl.w > 2 { "+" } else { " " },
                bl.w,
                bl.h,
                bl.x,
                bl.y,
                if info.new_point_blocks > 0 { format!("  cluster({} blocks, lost {})", info.new_point_blocks, info.new_point_cap) } else { String::new() },
                if info.exploded > 0 { format!("  EXPLODE cols {:08b}", info.exploded_cols.reverse_bits()) } else { String::new() },
                if info.refill_cap > 0 { format!(" refill +{}", info.refill_cap) } else { String::new() },
            );
        }
        for p in 0..4 {
            if info.exploded_cols >> (2 * p) & 3 != 0 {
                expl[wi][p] += 1;
            }
        }
        made[wi][(bl.x / 2) as usize] += 1;
        refill[wi] += info.refill_cap;
        let n = base + k + 1;
        if !list && n >= from && n <= to && (n % every == 0 || k + 1 == moves.len()) {
            print!("{n:>5} |");
            for p in 0..4 {
                let c: Vec<String> = clusters(&b, p).iter().map(|v| v.to_string()).collect();
                print!(" {:<38}|", c.join(" "));
            }
            print!("  cap");
            for p in 0..4 {
                let (cap, pa, void) = pipe_stats(&b, p);
                print!(" {cap:>4.1}/{pa:>2}/{void}");
            }
            println!();
        }
        if render == Some(n) {
            println!("{}", b.render());
        }
    }
    println!("\nper {window} moves: explosions in pipe 0..3 | splits made in pipe 0..3 | refill capacity | plugs made in pipe 0..3 | burials in pipe 0..3");
    for (wi, e) in expl.iter().enumerate() {
        let lo = wi * window;
        if lo + window < from || lo > to || lo >= moves.len() {
            continue;
        }
        let (pl, bu) = (plugs[wi], buries[wi]);
        println!(
            "{:>5}-{:<5} {:>3} {:>3} {:>3} {:>3} | {:>3} {:>3} {:>3} {:>3} | {:>4} | {:>2} {:>2} {:>2} {:>2} | {:>2} {:>2} {:>2} {:>2}",
            lo,
            (lo + window).min(moves.len()),
            e[0], e[1], e[2], e[3],
            made[wi][0], made[wi][1], made[wi][2], made[wi][3],
            refill[wi],
            pl[0], pl[1], pl[2], pl[3],
            bu[0], bu[1], bu[2], bu[3]
        );
    }
}
