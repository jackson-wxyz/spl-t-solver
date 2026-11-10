# SPL-T solver

An attempt to find very long games of Simogo's *SPL-T* (2015). The score is the number of
splits made before no split is possible. Goal: understand what optimal play looks like and,
ideally, beat the world record.

## Status (2026-11-10, session 6)

| | splits |
|---|---|
| Repo owner's personal best (by hand) | 477 |
| First beam search with hand-built evaluation (session 1) | 522 (`runs/best/scratch_522.txt`) |
| Josh Holder's best computed + hand-tuned sequence (2022) | 1208 |
| 2022 world record: Jonathon Becker (@mangosquash), by hand | 1277 |
| Best score posted publicly: ryederek on Bluesky, March 2026 (Legacy Collection) | 1910 |
| Rollout beam, from scratch (`rbeam`, width 100, k 4, cap 200, seed 3, ~40 s on the desktop) | 1885 (`runs/best/scratch_1885_rbeam_w100_k4_r200_s3.txt`) |
| Rollout beam, from scratch, two stochastic rollouts per child (width 100, k 4, cap 200, `--stoch 2`, seed 1, ~100 s) | 2064 (`runs/best/scratch_2064_rbeam_w100_k4_r200_st2_s1.txt`) |
| Ratchet from the 2064 game (re-search the best games' tails with fresh seeds and varied settings; about a day of search) | 2670 (`runs/best/ratchet_2670.txt`) |
| Iterated local search from the 2670 game (`ils`: kick the game, complete it by following its own schedule, polish with guided search; first hours) | 3012 (`runs/best/game_3012.txt`) |
| **The same local search with the off-guide rule** (`--gres 12`: a rollout that has to leave the game's schedule keeps off the blocks the game is about to split; 11 minutes from the 3012 game) | **3103** (`runs/best/game_3103.txt`; hand-replay data `runs/best/game_3103.json`) |
| Rollout beam resuming Josh Holder's 1208 game at move 700 | 1984 (`runs/best/from_josh1208_at700_1984_rbeam_w100_k4_r300.txt`) |

The engine replays all four published record games move for move, and every sequence in
`runs/best/` is re-verified by `replay`, which checks the engine's invariants after every
move. The best game has also been replayed in Josh Holder's original Python emulator
(`tools/analysis/pyreplay.py`), which accepts every move and ends on the last one. What has **not** been done is playing a solver sequence on the real game: a record
claim needs that, and the sequences are block-index lists in engine order, so it needs the
human replay tool listed under "Next steps".

## Prior art (the key find)

- **Craig Polley** wrote a Python SPL-T emulator and found a **1027**-split game (the 2018
  record) with brute-force random play over days of compute.
  [Blog post](http://www.flashingleds.net/brute_SPL-T/brute_spl-t.html).
- **Josh Holder** built on that emulator. A hand-tuned move-scoring heuristic plus a
  backtracking search ("ParanoidPlay") found 1053 splits alone, 1133 with human help, then
  1196; adding human edits took it to 1208. His write-up says he **took the world record
  with 1196**, which means his emulator's sequences were played on the real game, and that
  is what makes the emulator a trustworthy rules oracle. He also tried
  AlphaZero-style RL (`josh-holder/nn_split`) and found it far too slow to learn.
  [Blog post](https://josh-holder.github.io/projects/splt-blog),
  [code](https://github.com/josh-holder/SPLT). He guessed the practical ceiling is about
  1800 and the theoretical one about 3700.
- **Jonathon Becker (@mangosquash)** then took the record to 1258 and 1277 (2022). According to
  the Dec 2025 Bluesky thread he did it without computer help.
- **ryederek** (Bluesky), who made the record-history plot, posted 1396 splits on
  2026-03-10 and **1910 on 2026-03-20** on the Legacy Collection. Whether with computer
  help is not stated.

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

- `src/engine.rs` - fast game engine (~1M moves/s single-threaded on the laptop, late-game
  positions). The block order matches the reference emulator, so its move-index sequences
  replay directly.
- `src/heuristic.rs` - faithful Rust port of Josh Holder's move-scoring heuristic
  (`weighting.py`), checked weight-for-weight against his Python at all 4,590 record
  positions. Also the rollout policies built on it (greedy, avoid-big, softmax-sampled).
- `src/pipes.rs` - pipe-level structure (cluster values per 2-column pipe, "twins" count,
  whether a move plugs a pipe or buries free blocks, a health score for a pipe).
- `src/bin/ils.rs` - **the current best solver**: iterated local search on finished games.
  Each step kicks a game (plays a different move somewhere in its last few hundred moves and
  completes it by following the original), then polishes the result with a guided beam
  (rollouts replay the game's own moves wherever the same block still exists), and keeps
  it if it is longer. A step takes about a second. See `docs/RESEARCH_LOG.md`, "Session 5"
  and, for `--gres` (what a rollout does when it has to leave the schedule), "Session 6".
- `src/bin/abtest.rs` - paired comparison of search settings on tail re-search attempts.
- `src/bin/pipewatch.rs` - what each of the four pipes is doing along a game: cluster
  values, free capacity, explosions and splits per pipe, and the moves that wreck a pipe.
- `src/bin/whatif.rs` - replace one pipe of a position by a stack of your choosing and run
  the search from there (`ils --startboard`), to measure what a different middle game would
  be worth. The result is a measurement, not a legal game.
- `src/bin/doom.rs`, `nbhd.rs`, `polcmp.rs`, `opening.rs`, `refills.rs` - analysis tools:
  the capacity bound against moves actually left; the single-deviation neighbourhood of a
  game; play-out policies compared by survival; opening statistics; which refill situations
  a game relies on (to check before replaying it on the real game).
- `src/bin/ratchet.rs` - the session 3-4 solver: repeatedly re-searches the tail of
  the best games found so far with `rbeam`'s search and keeps anything at least as long.
- `tools/queue/` - job queue for long runs (`runner.sh`, `summary.py`).
- `src/rollout_beam.rs`, `src/bin/rbeam.rs` - beam search over the heuristic's top-k
  moves, each child scored by how long a greedy rollout survives from it. Flags: `--width`,
  `--k`, `--rollout` (cap), `--seed` (deterministic score noise, for several samples per
  setting), `--policy greedy|avoidbig|both`, `--stoch N --temp T`, `--wtwin`, `--prefix FILE
  --keep N`, `--out DIR`.
- `src/bin/paranoid.rs` - port of his ParanoidPlay backtracking search (reproduces his log).
- `src/bin/josh_play.rs` - greedy play with the heuristic; `--agree <seq>` reports how often
  the greedy choice matches a sequence. `src/bin/josh_dump.rs` prints weights in the
  reference's dump format for diffing. `src/bin/dbg_rollout.rs` / `dbg_policy.rs` are
  diagnostics.
- `src/search.rs` - session-1 position evaluation (passive look-ahead features) and the
  parallel beam search used by `solve`; `rbeam` borrows its `survive` feature as a tie-break.
- `src/bin/solve.rs` - the session-1 solver. `src/bin/replay.rs` replays and verifies a sequence.
  `src/bin/trace.rs` prints evaluation features along a game. `src/bin/bench.rs`
  measures speed and a random-play baseline.
- `src/bin/sweep.rs` - compares evaluation configs (one per line of a file, see
  `sweeps/example.txt`) on a fixed benchmark: a fresh game plus resuming the 1208 record
  game at moves 500/700/900. All jobs run in parallel.
- `src/bin/prof.rs` - per-operation timing on real positions. Build with `--features prof`
  for a per-phase breakdown of move resolution.
- `src/bin/selfcheck.rs` - equivalence check. It prints engine-state and evaluation digests
  for thousands of positions. Diff its output between a normal build and a
  `--features reference` build (simple whole-board versions of the optimized steps); they
  must be identical.
- `reference/` or `josh holder SPLT reference/` (both git-ignored) - clone of
  josh-holder/SPLT, needed for the record sequences (`sequences/1208seq.txt` etc.):
  `git clone https://github.com/josh-holder/SPLT reference/josh-holder-SPLT`. The desktop
  has it at `josh holder SPLT reference/SPLT/`. His Python runs with `python -I` and is
  the oracle for the heuristic port.
- `sweeps/` - config lists for `sweep`.
- `runs/best/` - best sequences found. A sequence is a list of block indices in the
  engine's block order. It's not yet human-friendly; see "Next steps".
- `tools/replay.html` - single-file page for replaying a sequence by hand on the real game:
  shows each board with the block to tap highlighted. It loads the per-move JSON written by
  `src/bin/export_game.rs`.

## Running

Run these from the project folder. Requires Rust (`rustup`).

```
cargo build --release
cargo run --release --bin rbeam -- --width 100 --k 4 --rollout 200 --seed 3
cargo run --release --bin ils -- --start runs/best/game_3103.txt --hours 1 --beam 0.05 --gres 12 --out runs/ratchet/try
cargo run --release --bin pipewatch -- runs/best/game_3103.txt --from 2300 --every 40
cargo run --release --bin rbeam -- --width 100 --k 4 --rollout 300 --prefix reference/josh-holder-SPLT/sequences/1208seq.txt --keep 700
cargo run --release --bin replay -- runs/best/scratch_1885_rbeam_w100_k4_r200_s3.txt --final
cargo run --release --bin paranoid -- --secs 60
cargo run --release --bin josh_play -- --agree reference/josh-holder-SPLT/sequences/1053seq.txt
cargo run --release --bin solve -- --width 800
cargo run --release --bin sweep -- sweeps/example.txt --width 200
```

`rbeam` is deterministic for a given set of flags; `--seed N` (N > 0) adds tiny score noise
so the same setting gives a different game. Results vary a lot between seeds (1440 to 1885
at width 100 / k 4 / cap 200), so run several. On the Ryzen 5 7600 a width-100 run takes
30-60 s; cost scales with width x k x rollout cap.

`solve` flags: `--width` (beam width), and evaluation weights `--ws --wd --tau --wc --wi --wdebt
--wslot --wmodel --mh --wback --wstuck --stuckt --alt`. See `Params::from_args` in
`src/search.rs`. The search is deterministic: the same flags always give the same game.

Speed on the laptop (i7-8565U, 4 cores): width 200 from scratch takes ~2-3 s, width 800 ~12 s.
Most of the time goes into evaluating positions; see "Performance" in
[docs/RESEARCH_LOG.md](docs/RESEARCH_LOG.md).

## Replaying a sequence by hand

```
cargo run --release --bin export_game -- runs/best/scratch_2064_rbeam_w100_k4_r200_st2_s1.txt runs/best/scratch_2064.json
```

Open `tools/replay.html` in a browser and load the JSON with "Open file" or by dropping it on
the page. Tap the highlighted block in the game, then press "I made this move" (or Space / Right
arrow; Left arrow goes back). The "Score" box jumps to the position where the game shows that
score, and the page remembers each file's position in the browser. Served over http, it also
takes `?src=`: run `python -m http.server` in the project folder and open
`http://localhost:8000/tools/replay.html?src=../runs/best/scratch_2064.json`.

## What I learned about the search

### Session 2: the evaluation was the problem, and rollouts fix it

Josh Holder's heuristic is 98% of his record play (the greedy choice matches the record
move at 98-99% of positions), but greedy alone is brittle: from arbitrary record positions
it dies within ~100 moves. Scoring each beam child by **how long a greedy rollout survives
from it** turned the 522-split beam into 1885 from scratch and 1984 from move 700 of the
1208 game, at width 100. Details, the cap sweep and the failure analysis ("twins" in a
pipe) are in [docs/RESEARCH_LOG.md](docs/RESEARCH_LOG.md).

### Session 1 (hand-built evaluation)

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

The full story of what was tried, what failed, and why is in
[docs/RESEARCH_LOG.md](docs/RESEARCH_LOG.md).

- **Human-friendly replay tool**: an HTML page that shows the board and highlights the next
  block to tap, so a sequence can be followed on the phone or the Steam version. This is
  now the blocker for a real record attempt.
- **More compute on `rbeam`**: wider beams and many seeds at cap 200; endgame re-search of
  the best game (`--prefix ... --keep N`).
- **Better rollout policy**: the greedy rollouts die by stacking "twin" clusters in a pipe.
  A policy that refuses to place a cluster on a pipe whose top cluster has not been halved
  yet, or heuristic constants re-tuned for the late game (values above 1200 are outside what
  the heuristic was tuned on), should make rollouts more informative.
- **Learned evaluation on the desktop GPUs**: the beam now produces thousands of strong
  games cheaply, which is the training data a small CNN value/policy net needs.
