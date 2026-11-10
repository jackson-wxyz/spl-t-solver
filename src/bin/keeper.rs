//! Pipe keeper: continue a game from move M with a beam that scores positions by the number
//! of levels the four pipes hold (see `src/keeper.rs`).
//!
//! usage: keeper GAME --from M [--width W] [--share F] [--seed S] [--until N] [--loose 1]
//!               [--wover X] [--wjunk X] [--wcap X] [--log N] [--out FILE] [--startboard FILE]
//!
//! Writes the whole game (GAME's first M moves, then the keeper's) to FILE and prints its
//! length. With --startboard the game starts from that position and GAME may be omitted
//! (pass "-"); the output then holds only the keeper's moves.
use splt::engine::{parse_moves, Board};
use splt::keeper::{keep, run, score, KCfg, PCfg};
use splt::rollout_beam::{arg, write_sequence};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    splt::rollout_beam::load_start(&args);
    let d = KCfg::default();
    let cfg = KCfg {
        width: arg(&args, "--width").unwrap_or(d.width),
        share: arg(&args, "--share").unwrap_or(d.share),
        w_phi: arg(&args, "--wphi").unwrap_or(d.w_phi),
        w_rate: arg(&args, "--wrate").unwrap_or(d.w_rate),
        w_excess: arg(&args, "--wexcess").unwrap_or(d.w_excess),
        margin: arg(&args, "--margin").unwrap_or(d.margin),
        w_sync: arg(&args, "--wsync").unwrap_or(d.w_sync),
        w_near: arg(&args, "--wnear").unwrap_or(d.w_near),
        w_bound: arg(&args, "--wbound").unwrap_or(d.w_bound),
        w_over: arg(&args, "--wover").unwrap_or(d.w_over),
        w_junk: arg(&args, "--wjunk").unwrap_or(d.w_junk),
        w_cap: arg(&args, "--wcap").unwrap_or(d.w_cap),
        loose: arg::<u8>(&args, "--loose").unwrap_or(0) != 0,
        seed: arg(&args, "--seed").unwrap_or(0),
        log: arg(&args, "--log").unwrap_or(d.log),
        until: arg(&args, "--until").unwrap_or(0),
    };
    let mut b = Board::start();
    let mut prefix: Vec<u8> = Vec::new();
    if args[1] != "-" {
        let all = parse_moves(&std::fs::read_to_string(&args[1]).expect("game file"));
        let from: usize = arg(&args, "--from").expect("--from M");
        for &m in &all[..from] {
            b.play(m).expect("illegal move in GAME");
            prefix.push(m as u8);
        }
    }
    let t0 = std::time::Instant::now();
    if arg::<u8>(&args, "--plan").unwrap_or(0) != 0 {
        // Planner mode: keep every pipe at its number of levels by depth-first planning,
        // then finish the game with the heuristic.
        let pd = PCfg::default();
        let pc = PCfg {
            horizon: arg(&args, "--horizon").unwrap_or(pd.horizon),
            commit: arg(&args, "--commit").unwrap_or(pd.commit),
            nodes: arg(&args, "--nodes").unwrap_or(pd.nodes),
            max_burn: arg(&args, "--maxburn").unwrap_or(pd.max_burn),
            sync: arg::<u8>(&args, "--sync").unwrap_or(0) != 0,
            seed: cfg.seed,
            log: cfg.log,
            until: cfg.until,
            cap_levels: match arg::<String>(&args, "--caplevels") {
                Some(t) => {
                    let v: Vec<u8> = t.split(',').map(|x| x.parse().expect("--caplevels a,b,c,d")).collect();
                    [v[0], v[1], v[2], v[3]]
                }
                None => pd.cap_levels,
            },
            guide: None,
            gain_nodes: arg(&args, "--gain").unwrap_or(0),
            gain_extra: arg(&args, "--gainextra").unwrap_or(pd.gain_extra),
        };
        // --follow 1: try GAME's own move first at every position (GAME must be given).
        let pc = if arg::<u8>(&args, "--follow").unwrap_or(0) != 0 && args[1] != "-" {
            let all: Vec<u8> = parse_moves(&std::fs::read_to_string(&args[1]).expect("game file")).iter().map(|&m| m as u8).collect();
            PCfg { guide: Some(splt::rollout_beam::guide_of(&all)), ..pc }
        } else {
            pc
        };
        let o = keep(&b, &pc);
        let mut game = prefix.clone();
        game.extend_from_slice(&o.moves);
        let mut t = Board::start();
        for &m in &game {
            t.play(m as usize).expect("planner produced an illegal move");
        }
        let kept = t.moves;
        let lv = splt::keeper::levels(&t);
        while let Some(&m) = splt::rollout_beam::top_k_moves(&t, 1).first() {
            t.play(m as usize).unwrap();
            game.push(m);
        }
        println!(
            "KEEP from {} horizon {} commit {} seed {}: planned to {} (levels {:?}, burnt {}, gained {:?}, gave up {:?}), heuristic finish at {} splits, {:.1}s",
            b.moves,
            pc.horizon,
            pc.commit,
            pc.seed,
            kept,
            lv,
            o.burnt,
            o.gains,
            o.losses,
            t.moves,
            t0.elapsed().as_secs_f32()
        );
        if let Some(out) = arg::<String>(&args, "--out") {
            write_sequence(&out, &game);
        }
        return;
    }
    let o = run(&b, &cfg);
    let mut game = prefix.clone();
    game.extend_from_slice(&o.moves);
    // Verify by replaying from the start position.
    let mut t = Board::start();
    for &m in &game {
        t.play(m as usize).expect("keeper produced an illegal move");
    }
    assert_eq!(t.moves, o.splits);
    let s = score(&t);
    println!(
        "KEEPER from {} width {} seed {}: {} splits (levels held until {}), final levels {:?}, {:.1}s",
        b.moves,
        cfg.width,
        cfg.seed,
        o.splits,
        o.held_until,
        s.per_pipe,
        t0.elapsed().as_secs_f32()
    );
    if let Some(out) = arg::<String>(&args, "--out") {
        write_sequence(&out, &game);
    }
}
