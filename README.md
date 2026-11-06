# SPL-T solver

An attempt to find very long games of Simogo's *SPL-T* (2015). The score is the number of
splits made before no split is possible. Goal: understand what optimal play looks like and,
ideally, beat the world record.

## Status (2026-11-06)

| | splits |
|---|---|
| Jackson's personal best | 477 |
| This solver, from scratch (beam width 800, ~1 min on the laptop) | **522** (`runs/best/scratch_522.txt`) |
| Josh Holder's best computed + hand-tuned sequence (2022) | 1208 |
| This solver, re-searching the last 108 moves of that 1208 game | 1209 (`runs/best/endgame_from_josh1208_at1100_1209.txt`) |
| World record: Jonathon Becker (@mangosquash), late 2022, by hand | **1277** |

## Prior art (the key find)

- **Craig Polley** wrote a Python SPL-T emulator and found a **1027**-split game (the 2018
  record) with brute-force random play over days of compute.
  [Blog post](http://www.flashingleds.net/brute_SPL-T/brute_spl-t.html).
- **Josh Holder** built on that emulator. A hand-tuned move-scoring heuristic plus a
  backtracking search ("ParanoidPlay") found 1133 → 1196 splits; adding human edits took it
  to 1208. He set the **1133 world record (Jan 2022) by replaying the computed sequence on
  his phone**, which is what makes his emulator a trustworthy rules oracle. He also tried
  AlphaZero-style RL (`josh-holder/nn_split`) and found it far too slow to learn.
  [Blog post](https://josh-holder.github.io/projects/splt-blog),
  [code](https://github.com/josh-holder/SPLT). He guessed the practical ceiling is about
  1800 and the theoretical one about 3700.
- **Jonathon Becker (@mangosquash)** then took the record to 1258 and 1277 (2022). According to
  the Dec 2025 Bluesky thread he did it without computer help.

## How the rules were pinned down

Of the approaches considered, existing open-source code turned out to be the right one:

- **Reviews/guides** (levelwinner, appspy) give the gist: alternating splits, 4 equal blocks
  become point blocks valued at the split count, and falling halves them. They're too vague
  on rounding, refill and cluster shapes to simulate from.
- **Gameplay video**: not needed. The rules are already validated over 4,590 moves of
  record play. It would only matter to confirm the 2025 Legacy Collection port uses the
  same rules.
- **Others' code**: Josh Holder's emulator. Its sequences were played on a real phone, and
  my Rust port replays all four published record sequences (1053/1133/1196/1208) move-for-move
  and ends each game on exactly the right move. See [RULES.md](RULES.md).
- **Steam/Android + computer use**: not needed to learn the rules. A record attempt still
  has to be played by hand on the real game, following a solver's sequence move by move.

## Layout

- `src/engine.rs` - fast game engine (~1.3M moves/s single-threaded on the laptop). The
  block order matches the reference emulator, so its move-index sequences replay directly.
- `src/search.rs` - position evaluation and parallel beam search (rayon).
- `src/bin/solve.rs` - the solver. `src/bin/replay.rs` replays and verifies a sequence.
  `src/bin/trace.rs` prints evaluation features along a game. `src/bin/bench.rs`
  measures speed and a random-play baseline.
- `reference/` (git-ignored) - clone of josh-holder/SPLT, needed for the record sequences:
  `git clone https://github.com/josh-holder/SPLT reference/josh-holder-SPLT`
- `runs/best/` - best sequences found. A sequence is a list of block indices in the
  engine's block order. It's not yet human-friendly; see "Next steps".

## Running

Run these from the project folder. Requires Rust (`rustup`).

```
cargo build --release
cargo run --release --bin solve -- --width 800
cargo run --release --bin replay -- runs/best/scratch_522.txt --final
cargo run --release --bin solve -- --width 400 --prefix reference/josh-holder-SPLT/sequences/1208seq.txt --keep 1100
```

`solve` flags: `--width` (beam width), and evaluation weights `--ws --wd --tau --wc --wi --wdebt
--wslot --wmodel --mh --wback --wstuck --stuckt --alt`. See `Params` in `src/search.rs`.

## What I learned about the search (2026-11-06)

1. **Beam width doesn't help yet.** Starting from the 1208 record game at move 900, beam
   widths from 50 to 4000 all die at 1035-1077. The record went on to 1208. Starting at
   moves 500/700/900, every evaluation variant tried keeps only 20-25% of the record's
   remaining moves. So the evaluation function is the bottleneck, not compute.
2. **Healthy play keeps the board full.** Along the record game, 90-120 of the 128 cells are
   point blocks almost the whole time, and the guaranteed buffer is only 20-40 moves. An
   evaluation that rewards free space and penalizes point-block area pushes the search the
   wrong way. A full board is what lets each new cluster sit on a tall stack and get halved
   many times.
3. **The failure mode is a fresh high-value cluster at the bottom.** For example, free blocks
   fall into a gap, form a cluster valued ~n, and nothing will ever halve it. That locks a
   quarter of the board for the rest of the game. The record player never allows it. The
   damage shows up beyond my look-ahead horizon, so the evaluation underweights it.
4. **Direction alternation can kill you even with "capacity" left**: e.g. every free block
   is 1 tall when a horizontal split is due. `alt_capacity` counts how many alternating
   splits are actually available.

The evaluation fast-forwards with no further splits (`passive_step`) to get the explosion
and halving schedule of the current point blocks. On top of that sit several features:
guaranteed survival, discounted capacity regained, the clear time of a hypothetical
cluster on each pipe (`slot`), and an abstract "pipeline" model of the refill → cluster →
halving feedback loop (`pipeline_model`). None of these has yet produced late-game play close
to the record's.

## Next steps (ideas)

- **Tune the evaluation from expert play**: fit the weights so that, at each position of the
  record games, the record's move scores higher than its siblings (conditional-logit /
  learning-to-rank, like tuning a chess evaluation from master games). Benchmark: extend
  record positions from moves 500/700/900.
- **Port Josh Holder's tactical heuristics** (`weighting.py`, "does this cluster gain a new
  halving") as move ordering or evaluation terms, or use his ParanoidPlay backtracking on top
  of the fast engine. His approach reached 1196 in pure-algorithm mode.
- **Learned value function on the desktop GPUs**: once there's a decent search, self-play data
  plus a small CNN over the 8×16 grid could replace the hand-written evaluation.
- **Human-friendly replay tool**: an HTML page that shows the board and highlights the next
  block to tap, so a sequence can be followed on the phone or the Steam version for a record
  attempt.
