//! What would a different pipe be worth? Take a game at move M, replace one pipe by a stack
//! of clusters of your choosing, and complete the game from that constructed position by
//! following the original game wherever it still applies.
//!
//! usage: whatif GAME --at M --pipe P --values v1,v2,... --out DIR [--gres 12]
//!        whatif GAME --at M --keep 1 --out DIR            (control: the position as it is)
//!
//! --values: cluster values for pipe P from the bottom up; the rows above them are filled
//!           with free 2x2 blocks. No block of the game may straddle the pipe's edge at M.
//! Writes DIR/start.txt (the position; pass it to `ils` or `rbeam` as --startboard) and
//! DIR/game0.txt (the moves of the completed game, counted from that position), and prints
//! the split count the completed game reaches.
//!
//! The result is not a legal game from the empty board. It is a measurement: run the local
//! search from it (`ils --startboard DIR/start.txt --start DIR/game0.txt ...`) and compare
//! with the same search from the unchanged position.
use splt::engine::{parse_moves, Block, Board, H};
use splt::rollout_beam::{arg, follow, guide_of, write_sequence, Config};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let moves: Vec<u8> = parse_moves(&std::fs::read_to_string(&args[1]).expect("game file")).iter().map(|&m| m as u8).collect();
    let at: usize = arg(&args, "--at").expect("--at M");
    let out: String = arg(&args, "--out").expect("--out DIR");
    let keep = arg::<u8>(&args, "--keep").unwrap_or(0) != 0;
    let cfg = Config { gres: arg(&args, "--gres").unwrap_or(12), ..Config::from_args(&args) };
    // The original game's schedule, by split count (must be built before the start changes).
    let guide = guide_of(&moves);
    let mut b = Board::new();
    for &m in &moves[..at] {
        b.play(m as usize).expect("illegal move");
    }
    let mut blocks: Vec<Block> = b.blocks().to_vec();
    if !keep {
        let pipe: usize = arg(&args, "--pipe").expect("--pipe P");
        let values: Vec<u16> = arg::<String>(&args, "--values").expect("--values v1,v2,...").split(',').map(|x| x.trim().parse().expect("value")).collect();
        let (x0, x1) = (2 * pipe as u8, 2 * pipe as u8 + 2);
        for bl in &blocks {
            let inside = bl.x >= x0 && bl.x + bl.w <= x1;
            let outside = bl.x + bl.w <= x0 || bl.x >= x1;
            assert!(inside || outside, "a block straddles the edge of pipe {pipe} at move {at}; pick another move");
        }
        blocks.retain(|bl| bl.x + bl.w <= x0 || bl.x >= x1);
        assert!(values.len() <= H / 2);
        for (k, &v) in values.iter().enumerate() {
            let y = (H - 2 * (k + 1)) as u8;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                blocks.push(Block { x: x0 + dx, y: y + dy, w: 1, h: 1, pts: v });
            }
        }
        let mut y = H - 2 * values.len();
        while y >= 2 {
            y -= 2;
            blocks.push(Block { x: x0, y: y as u8, w: 2, h: 2, pts: 0 });
        }
    }
    let start = Board::from_parts(&blocks, at as u16);
    if let Some(e) = start.check_invariants() {
        panic!("constructed position is inconsistent: {e}");
    }
    std::fs::create_dir_all(&out).ok();
    std::fs::write(format!("{out}/start.txt"), start.to_text()).expect("write start");
    println!("{}", start.render());
    let mut t = start.clone();
    let game = follow(&mut t, &guide, &cfg);
    write_sequence(&format!("{out}/game0.txt"), &game);
    println!("WHATIF at {at}: following the original game from the constructed position reaches {} splits ({} moves) -> {out}/start.txt, {out}/game0.txt", t.moves, game.len());
}
