# Research log & handoff notes

Written 2026-11-06 at the end of the first session (Claude Opus 5.5, on the repo owner's
laptop) for whoever picks this up next. That will probably be Claude Fable 5.1 on the
desktop (Ryzen 7600, 6 cores / 12 threads, two GPUs). It records what was tried, what
worked, what didn't, and what I'd do next. Read [README.md](../README.md) and
[RULES.md](../RULES.md) first.

## TL;DR

- **The rules are solved.** `src/engine.rs` is a Rust port of the Python emulator from
  [josh-holder/SPLT](https://github.com/josh-holder/SPLT). It replays all four published
  record sequences (1053, 1133, 1196, 1208 splits) exactly, and each game ends on exactly
  its last move. Josh Holder took the world record with 1196 (his write-up), so his
  emulator's sequences were played on the real game. So treat the engine as ground truth, quirks included. It runs at about 1.3M
  moves/s on one laptop thread.
- **Session 1's search was not the answer; session 2's is.** Beam search with hand-built
  evaluations reached 522 from scratch. Session 2 (below, "Session 2") replaced the
  evaluation with *rollouts of Josh Holder's ported heuristic*: **1885 from scratch** at
  beam width 100 in under a minute, and 1984 resuming his 1208 game at move 700. The best
  score posted publicly is **1910** (ryederek on Bluesky, 2026-03-20, on the Legacy
  Collection; whether by hand or with a computer is not stated), after 1396 on 2026-03-10;
  before that the record was 1277 (Jonathon Becker / @mangosquash, by hand, 2022). Nothing
  from this solver has been played on the real game yet; `tools/replay.html` exists for
  that.
- **Best game: 3898** (session 7), found by a beam search with no rollouts at all (the
  "pipe keeper", `src/keeper.rs`). It scores a position by how many levels the four pipes
  hold and by how many explosions neighbouring pipes will share. From move 700 of the old
  line it keeps all four pipes working, makes them eight clusters deep by move 2512, and
  ends in two unison pairs. Two runs of eight seconds each. See "Session 7".
- Before that: the ratchet took a from-scratch 2064 game to 2670, and iterated local search
  with guided rollouts took that to 3012 (session 5), 3103 (session 6) and 3140 (the same
  search, five more hours). Those games run three pipes and use the fourth as a store. They
  are superseded, but "Session 5" and "Session 6" explain how a pipe works, which is what
  the keeper is built on.
- **The bottleneck was the evaluation function, not compute.** Session 1 proved it (beam
  widths 50-4000 from move 900 of the 1208 game all died at 1035-1077); session 2 fixed it.

## The game in one paragraph

The board is 8×16 and starts as one block. Each move splits a block in half, with the
direction forced to alternate H, V, H, V... (horizontal first). Four identical, aligned
blocks in a 2×2 arrangement become point blocks valued at the current split count `n`.
Every move, all point blocks count down by 1, and at 0 they explode. Blocks above fall,
and every point block that falls has its value halved (rounding down). Voids are refilled
from the top by quirky rules. The game is over when no block can be split in the current
direction, and the score is the number of splits. Values grow linearly with `n`, so late
clusters are worth around 1000. The only way to clear them is to sit them on top of a stack
that explodes beneath them, halving them over and over.

## Timeline: research (how the rules were pinned down)

1. **Web reviews were thin and contradictory.** appunwrapper.com returned 403 and the
   Wikipedia page 404. levelwinner and appspy give the gist (alternation, "four identical
   blocks", value from the split count, halving on fall). But they disagree on "exactly
   four" versus "four or more", and on whether points come from splits or from blocks. Neither
   says anything about rounding or refill. That's not enough to simulate from.
2. **The Bluesky thread** (ryederek, 2025-12-06) only has the world-record history plot,
   and says "a couple of these people have written complex computer code". It has no links.
3. **GitHub search found two JS clones**, and they disagree with each other and with the real
   game: `tdanecker/spl-t` invents a refill rule and has halving where 1 → 0 destroys the
   block; `mlololoer/Javascript-SPL-T` rounds halving up and has no refill at all.
   **Lesson: hobby clones are unreliable.** Without refill the game can't exceed 127 splits:
   area is never created, so a binary split tree over 128 cells has at most 127 internal
   nodes. Any clone lacking refill is wrong.
4. **The breakthrough** came from a search for "SPL-T reddit high score strategy". It
   surfaced Josh Holder's blog post, which led to `josh-holder/SPLT` (the Python emulator,
   derived from Craig Polley's flashingleds.net one, plus record sequences) and
   `josh-holder/nn_split` (his abandoned AlphaZero attempt). His blog explains the strategy
   and says he took the world record with 1196 (1208 with human assistance). (Earlier
   versions of this log said 1133 was played on a phone; the blog does not say that.)
5. **Running his Python directly was blocked**: the sandbox's auto-mode classifier refuses
   to execute downloaded code. The workaround turned out better: port the rules to Rust,
   then validate by replaying his sequences. Each sequence is a list of indices into the
   emulator's block list, so the port keeps the exact same block ordering: a split shrinks
   the block in place and appends the new half, removal preserves order, and refills are
   appended.
6. **First port bug**: replay died at split 36. I had only run gravity after explosions.
   But a *vertical split of a block bridging a hole* leaves one half unsupported, and in the
   reference it falls on that move. That counts as "falling", which then triggers the
   post-fall cluster scan, halving, and full-height refill. After the fix, all four sequences
   (4,590 moves) replay exactly.
7. **Quirks deliberately preserved** (they match the real game): one-column void pockets are
   never refilled; odd-depth pockets leave the top row empty; if nothing fell but something
   exploded, only rows where explosions happened are refilled, so older voids wait until
   something falls through them; clusters formed by falling get value `n` and no halving;
   halving is floor, and 1 stays 1. A comment in `core.py` says "rounds UP", but the code
   floors and Josh's blog agrees with the code. The code is authoritative.
8. **One unresolved corner case**: `core.py` and `core_update.py` detect clusters slightly
   differently after a split (see RULES.md). The port follows `core_update.py`, which
   generated the record sequences. They can only diverge after a refill that happened
   without anything falling. Never observed to matter.
9. **Video analysis and Steam/Android computer use were skipped.** They weren't needed once
   the record replays validated the engine. `yt-dlp` and OpenCV were installed in a scratch
   venv but never used. The one remaining reason to watch footage is to confirm that the
   2025 Simogo Legacy Collection port uses identical rules.

## Timeline: engineering

- Engine v1: 0.76M moves/s. v2 caches the cell grid in `Board`, runs gravity only if
  something exploded or a vertical split bridged a hole, and runs refill only if the top
  row has a void. That got 1.33M moves/s with identical random-game statistics (same RNG,
  same histogram), so behaviour didn't change.
- `Board` is a fixed array (128 blocks + 128-byte grid, about 900 bytes), so cloning is a
  memcpy. Bitmasks are `u128` over block indices.
- Random play: mean 96 splits; best of 200k games was 302. Craig Polley needed something
  like hundreds of millions of random games to pass 1000, presumably with smarter sampling
  (his blog has details).
- `passive_step()`: fast-forward to the next explosion with no splits (count everything
  down by the minimum value, explode, fall, refill, halve). It's event-driven, so a look-ahead
  costs about (number of distinct explosion times) × (one move).
- The beam search (`search.rs`) is deterministic. Children are generated in parallel with
  rayon, deduplicated by an order-independent position hash, and the top W are selected
  with `select_nth_unstable`. Only (parent, move) history is stored, and selected children
  are recreated by replaying the move.

## Timeline: search experiments (all scores in splits)

Beam width W. Unless noted, the evaluation is `survive + 0.5·disc + 0.3·cap − 0.5·imb`
(`horizon` 400, `tau` 60).

| Experiment | Result |
|---|---|
| v1 eval, W=20 / 200 / 2000 | 229 / **402** / 317, so wider was *worse* (first sign of eval exploitation) |
| + point-area×time "debt" penalty 0.03 / 0.1 / 0.3 / 1.0 (W=200) | 320 / 354 / 327 / 268 |
| **Trace of the 1208 record game** | the record keeps 90-120 of 128 cells as point blocks, only 20-40 moves of guaranteed buffer, and *high* debt. My eval had the sign intuition backwards: free space isn't health |
| + `slot` (clear time of a hypothetical fresh cluster on each 2-column pipe) 0.2 / 0.5 / 1.0, debt 0 | 412 / 408 / 377 |
| same, but with capacity weight 0 | **131**. Capacity matters as a short-term safety term |
| slot 0.2, W = 50 / 100 / 400 / 800 / 1600 | 426 / 352 / 426 / **522** / 451, noisy and non-monotone |
| Beam from record positions (slot 0.2, W=400): from move 1100 / 1000 / 800 | **1209** (record 1208) / 1058 / 961 |
| Pipeline model (`pipeline_model`, see below) weight 1 / 3, others default, W=200 | 218 / 397 |
| Pipeline model with horizon 500 | **54**. The beam built an 8×8 cluster of 4×4 blocks; every remaining free block was 1 tall when a horizontal split was due. The capacity count was fine but nothing could be split in the required direction |
| + `alt_capacity` (greedy count of strictly alternating splits available) | 265-412, no clear gain |
| **Prefix benchmark** (resume at moves 500/700/900 of the 1208 game, W=200; record continues 708+508+308 = 1524 more moves) | every config gets 320-390 of those 1524 |
| From move 900, W = 50 / 200 / 1000 / 4000 | 1035-1077 for all; **flat in width** |
| + "stuck area" penalty (point area still present after 250 passive moves) | no improvement |

Note: `1196seq` and `1208seq` share the first 900+ moves, so a prefix benchmark over both
only has 3 distinct positions.

## Performance (second pass, same day)

Profiling (`prof`) showed about **95% of search time is evaluation**, and almost all of that
is the no-splits fast-forward (`passive_step`): about 20 explosion events per evaluation, at
3-4.5 µs each in the late game. A real move costs about 1 µs. Inside an event, every phase
walked the whole block list (100+ blocks late in the game, since each 1×1 cluster is four
blocks): gravity 40%, removing exploded blocks + redrawing the grid 34%, post-fall cluster
scan 14%, countdown 8%.

Exact optimizations made (no change to any result):

| Change | Why it's exact |
|---|---|
| **Gravity: one bottom-up pass over "active" columns only** (columns with an explosion or a hanging split half, plus columns of anything that falls) instead of repeated passes over every block | The settled state is unique: blocks never pass each other, and a block's resting place depends only on the final positions of the blocks beneath it. The board is settled before each move, so support can only disappear in active columns |
| **Grid remap instead of full redraw** after removing exploded blocks | Same grid contents; just renumbered |
| **Post-fall cluster scan only around blocks that moved or were just refilled** | A new 2×2 group must contain a block whose position changed. A `dirty_groups` flag falls back to the full scan in the one corner case where an unclustered group can survive (refill with no fall, where the reference doesn't scan) |
| **Cached next-explosion time; capacity and point area updated incrementally** from `MoveInfo` instead of re-summing all blocks per event | Arithmetic identity; checked by `debug_assert`s and by `check_invariants` after every replayed move |
| **Beam: hash and deduplicate children *before* evaluating them**, then evaluate the distinct positions as one flat parallel list | Equal positions have equal evaluations. The sort/dedup/select steps see the same keys in the same order, so they pick the same nodes |

How it was verified:

- `selfcheck` digests over 3000 random games plus every child of every position of the
  1133, 1208 and 522 games (about 10 MB of hashes and exact feature bits) are byte-identical
  across three builds: the optimized engine, the `--features reference` build, and the
  previous commit's engine.
- All record replays still pass, with stronger invariants (cache consistency, and no
  unclustered group while the flag is clear).
- `solve --width 200/800` produce byte-identical sequences to before (412 and 522).

Result on the laptop: **width 200 went from 13.2 s to ~2-3 s, and width 800 from 60 s to
~12 s** (about 5×). A fast-forward event costs ~1.5 µs now. Duplicate children are only
~12%. Most of the beam-loop gain came from load balancing: before, each rayon task
evaluated all ~30 children of one parent in sequence, and now evaluation is a flat parallel
list.

**Threads.** 1 / 2 / 4 / 8 threads take 6.9 / 4.3 / 2.8 / 2.8 s at width 200. The laptop has
4 physical cores and a 15 W power limit, so all-core clocks drop well below the single-core
boost. That's a hardware plateau, not a serial bottleneck: per-level serial work is trivial.
The Ryzen 7600 (6 cores, 65 W) should scale much better. For parameter sweeps,
`sweep` runs every (config, start) job in parallel, which keeps all cores busy.

**What's left, if speed matters again:**

- The remaining per-event cost is spread out: gravity ~40% (the inherent work of moving
  ~28 blocks when a stack drops), remove+remap ~22%, cluster scan ~18%, countdown ~12%.
- Struct-of-arrays point values plus SIMD countdown/explode detection might give
  another ~1.3×.
- *Sharing* fast-forward work between siblings doesn't pay off. Splits happen at the top of
  pipes, which are exactly the blocks that fall and get refilled within the look-ahead.
- Shortening the look-ahead horizon would help a lot but changes the evaluation, so treat it
  as a tuning parameter, not a free speedup.
- **GPUs** suit a learned evaluation (batched network inference over the beam's thousands of
  children) far better than the engine itself. The engine is branchy, with irregular
  per-board loops.

## Resume here (session 7, 2026-11-10 afternoon: the pipe keeper, 3898)

- **Best verified game: 3898** (`runs/best/game_3898.txt`; hand-replay data next to it as
  `.json`). Its first 700 moves are the opening every game since the from-scratch 2064 game
  shares. Moves 701 to 2600 come from
  `keeper runs/best/game_3103.txt --from 700 --width 300 --seed 2 --wcap 30` (a 3588 game,
  kept as `runs/best/game_3588.txt`), and the rest from
  `keeper runs/best/game_3588.txt --from 2600 --width 1000 --seed 2 --wsync 60`. Checked by
  `replay`, by the engine's slow reference build, and by Josh Holder's Python emulator
  (`tools/analysis/pyreplay.py`): 3898 of 3898 moves accepted, game over on the last one.
- **Nothing is running.** The session-6 local-search workers were stopped at 15:18 (they
  had reached 3128 and 3140 on the old line). Results of the keeper grids are in
  `runs/keeper/grid*.log` (one line per run); the games of 3,600 splits and more are in
  `runs/keeper/grid3/` and `grid4/` (git-ignored).
- **To finalize after a run**: copy the game into `runs/best/`, then
  `bash tools/queue/finalize.sh` (verifies the longest game in `runs/best/` and
  `runs/ratchet/*/best*.txt`, exports the replay JSON, rebuilds `tools/viz/data.js`), then
  republish `tools/viz/index.html` (artifact
  https://claude.ai/artifact/TFs2adq7eKa8naU47FX2Ag), update the numbers in the blog draft
  (its header comment lists every place), re-run `tools/analysis/record_history_chart.py`
  with the new `SOLVER`, and update the README table and the Asana card.
- **Before a real-game replay**: run `refills` on the game. For the 3898 game the four
  refill situations its late game is made of (four cluster blocks to a 2x2, eight to a 4x2,
  the two blocks of half a wide cluster to a 2x2, the four blocks of a wide cluster to a
  4x2) all occur in the published 1053, 1133 and 1196 games, which were played on the real
  game. The rarer ones before move 900 are those of the old opening (see "Session 5").
- **Next research targets**, most promising first. (1) The start: nine keeper runs in ten
  from the opening die within 150 moves, because the position they take over holds blocks
  the keeper refuses to use (see "Session 7"). A keeper that may spend that junk, or an
  opening made for it, would make every run count and allow other openings. (2) The
  ending: from move 3300 every setting ends on exactly 3898, 188 moves past the
  balance point of two pairs, and the local search finds nothing there. Whether a larger
  reserve at the balance point is possible is open. (3) The capacity a game burns to keep
  its depth (764 splits in the 3898 game) cannot be stored on a full board, but the first
  half of it is burnt before move 1700, when a shallower board would do.
- Public best to beat: 1910 (ryederek, Bluesky, 2026-03-20).
- Blog draft: `Personal Blogs/jwxyz/content/Posts/spl-t-solver.md` (Hugo, draft: true, purely
  technical). Video plan: Asana card "SPL-T world-record video".

## Session 7 (2026-11-10 afternoon, desktop, Claude Fable 5.2): the pipe keeper, 3140 to 3898

Jackson's instruction: build the scripted fourth-pipe schedule that session 6 ended on. It
was built three times. The version that works is the simplest one.

### What changed

- **`src/keeper.rs`, `keeper` binary: the pipe keeper.** A beam search over all legal moves
  that never plays a game out. A position is scored by counting, per pipe, the separate
  explosions its stack will produce if nothing is put under it (`events`, an exact
  simulation of the countdown and halving rules on the stack's values), plus one for a pipe
  whose top two rows are waiting to become the next cluster. That count is the pipe's
  number of levels, 8 at most. The beam keeps the positions with the most levels in total.
  Ties go to less overdue work (`over`), and positions that cannot reach their next
  explosion on the capacity and split directions in hand come last (`doomed`). Moves that
  make a cluster anywhere but on top of a stack are refused while another move exists.
  Positions are grouped by how much capacity they have burnt so far and no group may take
  more than 40% of the beam (`--share`), so lines that burnt a wide cluster and lines that
  did not are both kept until one of them loses a level.
- **Unison bonus (`--wsync W`)**: W points for every explosion that two neighbouring pipes
  will have on the same move (their holes then refill as one 4x2 block worth 7 splits
  instead of 6). A level counts 1,000, so this only decides between positions with equal
  depth. It is the difference between 3,600 and 3,900.
- **A planner (`keeper --plan 1`, `keeper::keep`)**: depth-first search for 24 moves along
  which no pipe loses a level, with the least burnt capacity; six moves are played and it
  plans again. When no plan exists it gives up one level in one pipe. `--follow 1` makes it
  try a given game's own move first, `--caplevels a,b,c,d` limits what it holds per pipe,
  `--gain N` lets it try for an extra level. It was the second attempt and is only useful
  on tidy positions (see Results); it stays because it measures things the beam cannot,
  such as whether a given set of levels can be held at all.
- **`pipewatch --list 1`**: one line per move (direction, pipe, block, cluster made and
  capacity lost, explosions, refill). **`dbg_keeper`**: the keeper's view of every position
  of a game (levels, due pipes, work, deadlines).
- Score terms that were tried and are off by default: `--wcap` (capacity in hand; used in
  the first of the two runs behind the 3898 game, no measurable effect), `--wbound` (the
  capacity bound of session 5), `--wnear` (partial credit for explosions a few moves
  apart), `--wrate --wexcess --margin` (levels counted as return rate, capped at what the
  split count can pay for). See Results.

### Results

- **3550, then 3588, then 3898.** All three are legal games from the empty board and pass
  the three checks.
  - 3550: `keeper --from 1000 --width 1000` on the 3139 game, levels only. 15 seconds.
  - 3588: `--from 700 --width 300 --seed 2 --wcap 30`. 8 seconds.
  - 3898: `--from 2600 --width 1000 --seed 2 --wsync 60` on the 3588 game. 8 seconds.
  The first 700 moves are those of the from-scratch 2064 game of session 2 (a 100-second
  rollout-beam run), so nothing the ratchet or the local search found after move 700 is in
  the record game.
- **What the 3898 game does.** All four pipes are eight clusters deep from move 2512 to the
  end (before that it runs 27 to 31 levels in total and gains the missing ones one at a
  time). Pipes 0 and 1 explode on the same move from about move 2850. Pipes 2 and 3
  explode one move apart until about move 3300 and together from then on: two unison
  pairs, half a period apart. From about move 3640 the game is
  an exact fourteen-move cycle: each pair explodes every 14 moves and gets a 4x2 block that
  takes 7 splits, the two pairs seven moves apart. The capacity accounting is 3898 = 127 +
  4540 refilled - 764 lost to clusters - 5 left over. The 764 are wide clusters: per 200
  moves it burns 118 between moves 800 and 1000, about 40 around move 2000, 22 after move
  3000, and nothing after move 3600.
- **The end points follow the arithmetic of session 6.** A keeper game that gets through
  the middle game ends between the balance point of what it has built and about 200
  moves past it. 432 runs from moves 2300 to 3100 of the 3588 game (`--wsync` 20, 60 or
  150; widths 300, 1000, 3000) ended as follows; 64 more failed before move 3,300:

  | Final structure | Splits back per period | Balance point | Runs ending past it |
  |---|---|---|---|
  | Four single pipes | 12 | 3,180 | 33: 3,306 to 3,443 |
  | One pair and two singles | 13 | 3,445 | 301: 3,468 to 3,652 (82 at 3,612) |
  | Two pairs | 14 | 3,710 | 34: 3,712 to 3,898 (three at 3,898) |

  Runs are assigned to a row by where they ended; the structure itself was checked only
  for the 3898 game. From move 2600, eight seeds each (width 300): without the unison
  bonus 2882 to 3546 (median 3373), with `--wsync 5` median 3578, with `--wsync 20` 3500 to
  3626. At width 1000 with `--wsync 20`: 3306, 3612 four times, 3636 twice, 3638.
- **Most runs from the opening die at once.** Of 54 runs each from moves 700 to 1200 of the
  old game (levels only, three widths, six seeds, three values of `--wcap`), 2 to 11 got
  past move 2,500, and the rest ended within about 150 moves of their start. With
  `--wsync 60` from the start, 8 of 216 runs got past 3,300. From move 600 nothing
  survives; from move 1500 on, the old game's own structure (three pipes and a store)
  gets in the way and runs end between 2,100 and 2,700.
- **The planner on tidy positions** (the `whatif` constructions of session 6: the 3103 game
  at move 2600 with pipe 0 replaced by a clean stack). With a seven-deep pipe 0 it held all
  four pipes from move 2600 to 3086 (3110 with the unison rule), burning 48 splits in
  twelve wide clusters, in four seconds; session 6 put the balance point of that structure
  at 3,046. With an eight-deep pipe 0 it held four eight-deep pipes to 3254, burning 120
  (balance point of four singles: 3,180). From real positions it lasts 60 to 150 moves.
  The beam from the same seven-deep construction reached 3588 in 15 seconds and added the
  eighth level to pipe 0 by itself.
- **The old line**, before its workers were stopped: 3128 (`runs/ratchet/ils_h`) and 3140
  (`runs/ratchet/ils_i`; the keeper runs above started from its 3139 game). `lin_nc60` (the delayed-opening lineage) ended at 2646.

### Findings

- **How a pipe gains a level.** Session 6 had the first half: two clusters made in one
  interval that arrive at the bottom one apart, so that they explode on consecutive moves.
  That alone gains nothing. The cluster made just before the pair (A) is halved twice in two
  moves, the next one made after it is halved not at all, and the stack has a gap of a
  factor of four where a level should be. The level is gained only if another cluster (B)
  is completed on exactly the move of the second explosion: it is halved once, and A, B and
  the clusters after them are then spaced by factors of two. B has to be made in one move
  from a 2x2 block that appeared one move earlier. The games do it with a horizontal split
  that turns the block into two 2x1 blocks next to two 2x1 blocks the neighbouring pipe has
  kept waiting: a wide cluster across two pipes. The 3139 game does this at move 2397 (pipe
  1, with pipe 2; that wide cluster is also what put pipes 1 and 2 in unison) and at move
  2563 (pipe 3, with pipe 2). At move 2334 pipe 3's pair of explosions ended on an even
  move, which needs a vertical split, and the level was not gained. A pipe with two spare
  slots can do the same alone, by leaving a block one split short of a cluster and
  finishing it on that move.
- **Why the search's games are never tidy, and what that cost.** The rollout searches keep
  the board at balance: whatever the pipes return is spent, and anything left over goes
  into clusters in the wrong place, which come back hundreds of moves later. That store is
  how those games survive their deficits, and it is why one pipe is always a wreck. The
  keeper has no store. It runs more levels than the split count needs and burns the
  difference in wide clusters, so that when the count catches up the levels are there. The
  764 splits it burns are what the fourth pipe costs.
- **The first attempt failed because of one line.** The check for "a new cluster with a
  point block above it" counted the other blocks of the same new cluster, so every ordinary
  cluster was refused and the beam died within a hundred moves from anywhere. For an hour
  that looked like evidence against the idea, and the planner was written as the careful
  alternative. The beam was only retried, on a hunch, after the fix had gone in for the
  planner's sake.
- **Full depth too early kills.** A beam that only counts levels sometimes reaches four
  eight-deep pipes around move 1750 and starves at once: with a period of 6.6 moves a pair
  has no time for anything but wide levels, which return 3 splits per pair and period, so
  the game gets 6 splits per 6.6 moves. Four eight-deep pipes become possible at about
  move 1,860 (two pairs) or 2,400 (one pair). Scoring levels as return rate and capping the
  reward at what the split count can pay for (`--wrate 10 --wexcess 10`, margins 0.15 to
  0.5) did not help from move 1000: 1120 to 1357 over nine runs.
- **A quad cannot run by itself.** With all four pipes in unison the refill is one 8x2
  block, and the ways to turn it into clusters take 3, 7, 11 or 15 splits. The interval is
  whatever the cluster values make it, so it would have to be exactly one of those numbers
  every time. Two pairs half a period apart can lend each other moves; that is the largest
  structure that works, and its balance point of 3,710 is why the games stop near 3,900.
  (Whether a structure that is a quad only some of the time could do better is untested.)
- **The end is fixed 600 moves ahead.** From moves 3300, 3500 and 3700 of the 3898 game,
  every setting tried (unison bonus, capacity, capacity bound up to 1,000 per move, unclean
  clusters allowed, widths 1000 and 3000) ends on 3898 or earlier, never later. At the end
  there are 5 splits of capacity left in blocks that cannot be split in the direction due,
  and the next explosion is 4 moves away.
- **Nulls.** `--wnear` (partial credit for explosions up to 7 moves apart): medians near
  3,100 against 3,600. `--wbound 40` from moves 2300 to 2600: 2793 to 3114. Local search
  (`ils --gres 12` and `--gresl 6,12,24,40`) on the 3550 and 3898 games: no gain in 560 and
  400 iterations; kicked games end 178 moves short on average, because a rollout that
  follows the game move for move has no slack to rejoin a cycle this tight. The planner's
  extra-level attempts (`--gain`): the extra cluster gets made, and a level is lost
  elsewhere within 30 moves.

## Session 6 (2026-11-10 midday, desktop, Claude Fable 5.2): the fourth pipe, 3012 to 3103

Jackson's instruction: go after the fourth pipe (session 5's "obvious next target"). The
two workers left on the 3012 game had found one more game (3030) in six hours and 34,000
iterations, so the old search was finished there.

### What changed

- **Off-guide rule for guided rollouts** (`--gres N`, `rollout_beam::off_guide_move`).
  When the game being followed has no playable move for the turn, the rollout used to take
  the heuristic's first choice. That often split a block the game was about to split a few
  moves later, which then derailed the schedule of a pipe that was running fine. Now it
  takes the heuristic's best move on a block that none of the game's next N moves touches
  (in practice: a block in the pipe the schedule does not describe), failing that a later
  move of the game that can be played early, failing that the heuristic's choice. This is
  the change that produced the new record.
- **`pipewatch`**: cluster values per pipe along a game, free capacity and point area per
  pipe, and per window of moves how often each pipe exploded, how many splits it received,
  the refill capacity, and how many moves broke a pipe. `--render M` prints a board.
- **`pipes::breaks_pipe`**: does a move make a plug (a new cluster underneath a point
  block that has already been halved) or a burial (a new cluster on top of free blocks)?
  **`pipes::health`**: a score for how well a pipe is set up to keep cycling.
- **Clean-pipe rule** (`--clean M --cleanfrom T`): the beam, the kick and every rollout
  refuse plugs (bit 0) and burials (bit 1) from move T while another move exists.
  `--fclean` applies it to off-guide moves only.
- **`whatif`** and `--startboard FILE` (`ils`, `rbeam`): take a game at move M, replace one
  pipe by a stack of clusters of your choosing, and run the search from that constructed
  position. Not a legal game; a way to measure what a different middle game would be worth
  before trying to build it. (`Board::start`, `set_start`, `from_parts`, `to_text`.)
- Smaller options, all measured and none adopted as a default: `--gkid` (a guided beam
  always expands the game's own move), `--kfree N` (also expands, and kicks on, blocks the
  game leaves alone), `--whealth X --hpipe P --stopat T` (beam bonus for a healthy pipe,
  stop at move T and return the best-scoring line), `ils --gresl a,b,c` (draw the
  look-ahead per iteration).

### Results

- **3012 to 3044 in one 7-second guided beam** from move 2300 of the 3012 game with
  `--gres 12`. Eight seeds gave 3032 to 3078 (mean 3051); without the rule the same beam
  returns 3012. Look-aheads from 4 to 40 moves all work (3021 to 3066, one seed each).
- **`ils --gres 12`**: 3046, 3069, 3090 in the first 139 seconds and **3103** after 11
  minutes (`runs/ratchet/ils_g12`). All three checks pass on the 3103 game (capacity
  accounting: 3103 = 127 + 3156 refilled - 180 lost to clusters - 0 left).
- **Then it stalled again.** On the 3103 game, 80 paired kick-and-polish attempts gave no
  gain, and guided beams from move 2300 with all legal moves as children, look-aheads of 6
  to 32, or width 400 returned 3103 every time. Two workers continue on it.
- What the new games do differently is in pipe 0, the one the three-pipe cycle does not
  use. It still is not a working pipe. The search arranges its leftover clusters so that
  several explode within a move or two of each other (three around move 2812, two around
  2946),
  each time handing back a large free area exactly when the three working pipes fall short.

### Findings: how a pipe works, and why the fourth one dies

These come from reading games with `pipewatch` and from the engine's rules. The examples
are from the 3012 game.

- **A pipe is a delay line.** Each explosion at the bottom halves everything above, so a
  stack of L clusters whose top is worth the split count n settles at one explosion every
  n / (2^L - 1) moves: n/255 for eight clusters (the full height of the board), n/127 for
  seven. In the games the eight-deep figure is about n/265, as in session 5: halving
  rounds down and a cluster takes a few moves to make. Between explosions the pipe must be
  given exactly one new cluster. The time
  between two explosions at the bottom is the gap between the two clusters' values when
  the upper one was made, divided by 2 for every explosion since.
- **Supply and demand.** Every move spends one split and an exploding cluster returns three
  (a 2x2 refill), or seven per two clusters when two pipes explode together (a 4x2 refill).
  With eight-deep pipes exploding every T = n/265 moves, the splits returned per T are: 10
  for a unison pair plus one single pipe, 13 with a second single pipe, 14 for two pairs.
  The game needs T per T. So three pipes carry the game to about n = 2,650, four pipes with
  one pair to about 3,450, two pairs to about 3,700. A seven-deep fourth pipe adds 1.5 per
  T (about 3,050). After the balance point the game lives on stored capacity, which in
  these games lasts roughly another 15 percent. That is why the 3012 game ends where
  it does: pipes 1 and 2 are a pair, pipe 3 is a single, pipe 0 is not working.
- **A pipe can lose depth easily and regain it only by a trick.** Miss one cluster and
  the pipe is one level shallower from then on, with twice the period. Making two clusters
  in one interval does not repair it: their values differ by a few moves, the difference
  halves with every explosion below, and they arrive at the bottom equal and explode as
  one event. The exception is a pair that arrives exactly one apart: the upper one falls
  with value 1, the rule "1 stays 1" keeps it at 1, and it explodes on the next move. That
  is two events, and everything above has been halved twice. Pipe 2 does this at moves
  2182/2183 (clusters worth 39 and 40 at move 2160). Pipe 3 goes from six clusters to
  eight between moves 2360 and 2600 the same way: two clusters made six moves apart near
  move 2445 (so worth 12 apart) stay distinct all the way down (1214/1208, 601/598,
  289/287, 132/131, 53/52, 13/12, 3/2). Whether a pair survives is decided by the low bits of its value when it is
  made, so it can be planned; the search found these by selection.
- **Why pipe 0 dies.** From move 2400 the pair and pipe 3 return 10 splits per period and
  the period is about 10, so they use every move. Pipe 0 cannot be kept eight deep as
  well: four pipes would return 13 per 10 moves and only 10 can be spent. The games resolve
  this by using pipe 0 as a store: its free blocks get buried under clusters and are later
  split where they lie, which makes plugs (after move 2400 the 3012 game makes three plugs,
  all in pipe 0, and four burials, three of them in pipe 0). By move 2760 its bottom cluster is worth 557. The moves
  it receives per 100 are 12, 11, 0, 4, 8, 9 from move 2400 on, against the 15 a
  seven-deep pipe would need, and during the exact ten-move cycle (2600 to 2740) it gets
  none at all.
- **A single pipe cannot be throttled, a pair can.** A 2x2 refill can only become a
  cluster with three splits. A pair's 4x2 refill can become two clusters with seven splits
  or one wide cluster (four 2x1 blocks) with three. A wide level explodes like any other
  and the refill is again 4x2, so a pair can shed four splits in any period without losing
  depth. That is the mechanism that would make room for a fourth pipe before move 3,450:
  about four wide levels per 100 moves during the ten-move cycle (a seven-deep pipe 0
  needs 15 moves per 100). No game so far uses it
  (capacity lost to clusters is 180 splits in 3,103 moves, nearly all of it early). Note
  that a wide cluster does not tie two pipes together: each 2x1 block spans one pipe and
  falls with it. Only blocks four wide do that, and those exist only where two pipes
  already fell on the same move. All of this is deduced from the rules; no wide level has
  been tried in a search yet.
- **The plain fix does not work.** Forbidding plugs from move 2300 or 2380 (`--clean 1`)
  turns the guided search's 3012 into 2581 to 2641, with or without the off-guide rule, and
  forbidding burials too gives 2499 to 2532. Heuristic beams from cuts near move 2350 reach
  about 2,530 whether the rule is on or not (six paired attempts: +12 +/- 15 and +10 +/- 14
  moves). The store moves are what keeps the three-pipe cycle fed between moves 2450 and
  2600, when pipe 3 is still rebuilding its depth. A local search started in the clean
  space climbed 2591 to 2644 in seven minutes (`runs/ratchet/p4_c1k0`) and was stopped
  there.
- **What a working fourth pipe is worth to this search (`whatif`).** The 3103 game at move
  2750 with pipe 0 replaced by a clean staircase of six clusters (22, 65, 151, 324, 670,
  1361) under two free blocks, then the same local search from that position and from the
  unchanged one. The unchanged position stayed at 3103 (18 minutes, two threads). The
  constructed one started at 3019 (the old schedule knows nothing about the new pipe),
  passed 3103 after three and a half minutes, reached 3127 after nine, and found nothing
  in 16 more minutes on six threads. `pipewatch` shows why: the new pipe explodes on
  schedule for about 150 moves (two or three times per 50 moves), but the search plugs and
  buries it from the first 50 moves on, and by move 3060 its bottom cluster is worth 695.
  The same run with plugs forbidden from move 2750 reached 3057, with burials forbidden too
  3090, and with the rule applied to off-guide moves only 3125 (18 minutes, two threads
  each). So handing this search a working fourth pipe is worth about 25 moves. It cannot
  run one, with or without the rules: the pipe needs a schedule of its own (one cluster per
  explosion, on top of the stack), and room has to be made for its splits. An eight-deep
  version at move 2750 (5, 27, 70, ...) started at 3019 and went nowhere in 18 minutes, and
  the seven-deep construction at move 2600 was still short of the unchanged game after
  nine minutes (3011 and 3046 for two phases of the new pipe) when it was stopped.
- **Other nulls.** The clean rule for off-guide moves only: 3022, 3077, 3022 against 3044,
  3055, 3078. Trying a later move of the game before a free block: 3012 seven times out of
  eight. `--gkid`, `--kfree 3`, both, and `--kfree 8`: means 3057, 3055, 3048, 3060 against
  3051 over eight seeds (noise is about 15 per run). Kicks restricted to free blocks: -5
  +/- 7 against ordinary kicks, no gain from either. A beam that scores pipe 0's health
  (weights 5 to 40, stopped at move 2750) returns the game unchanged: during the ten-move
  cycle there are no spare moves for pipe 0, so no line in the beam keeps it healthy.

### Two ways to build the fourth pipe (not done)

(Session 7 did it a third way, with a beam that counts levels. It ends in two pairs, as
the second plan below proposes, without being told to.)

1. **A scripted schedule.** From about move 2360 follow the game for pipes 1 to 3, give
   pipe 0 one cluster per explosion on top of its stack, and make the pair's level wide
   whenever pipe 0 is waiting for moves. The pieces exist (time-aligned follower, off-guide
   rule, `breaks_pipe`); what is missing is the trigger for a wide level, which takes two
   H turns with the blocks left alone in between, so the plain follower cannot find it.
   The `whatif` runs say the search will not keep such a pipe alive afterwards, so the
   rule for pipe 0 has to stay in force to the end of the game, in the rollouts too, and
   `ils` can only be trusted with the other three pipes.
2. **Two pairs from the opening.** Treat the board as two pipes four wide. Each level is
   wide (three splits) or two ordinary clusters (seven), both halves made in the same
   interval, so each pair is in unison by construction and can be throttled at any n. The
   two pairs should explode at different times: with all four in unison every interval
   would need exactly 3, 7, 11 or 15 splits, and there is no slack.

## Session 5 (2026-11-10, desktop, Claude Fable 5.2): guided local search, 2670 to 3012

Jackson suggested five directions: CPU work, declaring a game lost early ("checkmate"),
learning a better evaluator from the long games, the worry that everything descends from
one game, and an opening that delays the first point blocks. This session tried all five.
The big result came from a sixth thing that the checkmate analysis led to.

### What changed

- **Guided rollouts** (`rollout_beam`, `--guide 1`). A rollout that plays the move the
  game being re-searched made at the same split count, whenever a free block with exactly
  that position and size exists, and the heuristic's move otherwise. From a position on
  that game it replays the rest of it, so the node's value is its true known value instead
  of a greedy rollout's guess. From a position that deviated, it tests whether the
  deviation can rejoin the game's schedule and what that does to the ending. A beam using
  only this rollout (`--policy none`) is a local search around the game: it can never
  return a shorter game, and a search step costs about 1 s instead of 5-50 s.
- **Ending cache** (`EndCache`): guided rollouts stop at the first position an earlier
  rollout already played out. A cached ending that would be a new best is replayed first,
  so results are exact.
- **`polish`**: guided-only beam rounds, each guided by the current game, until a round
  finds nothing longer. **`kick`**: play a different move at move t and complete the game
  by following the original.
- **`ils`** (iterated local search): pick a pool game, kick it somewhere 75-450 moves
  before its end, polish from the move after the kick, keep the result if it is longer
  (or, half the time, if it ties: a neutral step). With `--beam P` a fraction of iterations
  are ordinary rollout-beam re-searches of the tail, followed by the same polish.
- **`abtest`**: paired comparison of settings on tail re-search attempts (same parent, cut
  and seed for every arm). Everything below that says "paired" used it.
- **`doom`**, `search::passive_bound`: the capacity bound described under Findings.
- **`nbhd`**, **`polcmp`**, **`opening`** + `opening_corr.py`, **`refills`**: analysis tools
  for the experiments below. `rbeam --nocluster D` forbids cluster-making moves in the
  first D moves while another move exists.
- Heuristic factors are per search configuration now (no process global), so one process
  can compare several.

### Results

- **2670 to 2690** in one 22-second guided beam, after 9 worker-hours of the unguided
  ratchet had found nothing. The guided beam then returned 2690 every time: it converges
  to a local optimum in one or two attempts.
- **`ils` from the 2690 pool** (one worker, six threads): 2694, 2711, 2715, 2721 in the
  first 16 minutes, **2812** at 32 minutes (one kick, +91), 2821, 2855, **2963** (a beam
  attempt, +108), 3007 and **3011** at 37 minutes; then 3012 from a second run. All verified
  with `replay`, with the engine's slow reference build (`--features reference`), and by
  replaying the 3012, 3011 and 2670 games in Josh Holder's original Python emulator
  (`tools/analysis/pyreplay.py`): it accepts every move and the game ends on the last one.
- **What the 2812+ games do differently: two pipes fire in unison.** From about move 2415
  the two middle pipes hold identical staircases and explode on the same move (the
  ratchet's games already had those two pipes within a few counts of each other at move
  2400; the local search turned that into an exact, lasting cycle). Their two
  2x2 holes then refill as one 4x2 block, worth 7 splits instead of 6. With the third pipe
  firing one move earlier (3 splits), the three pipes return exactly 10 splits every 10
  moves: a cycle that repeats move for move, with the staircase 2 / 23 / 65 / 148 / 317 /
  654 / 1332 / 2690 satisfying v(k+1) = 2 (v(k) + 10) at every level. The fourth pipe
  holds stuck clusters and fires only occasionally. The session-2 bound analysis listed
  bigger refills as the way past about 3,200; this is the search finding that by itself.
  The 4x2 refill after a paired explosion also occurs 12 times in Josh Holder's 1196 game,
  which was played on the real game, so the rule the cycle depends on is a tested one.
- **Why it stops near 3,000, and what would go further.** A pipe eight clusters deep must
  fire every n/265 moves. The three live pipes supply exactly 10 splits per cycle, which
  matches n = 2,650; by n = 3,000 the pipes want 11.3 and the game is living off the fourth
  pipe's occasional explosions. The exact ten-move cycle lasts from about 2,600 to 2,740
  and then stretches until it breaks. With the fourth pipe healthy the same structure would
  supply 13 per cycle (good to about n = 3,450), and two unison pairs 14 (about 3,700). In
  this lineage the fourth pipe turns into a stack of stuck clusters between moves 2,380 and
  2,560, so a longer game needs a different history from about move 2,350 on. That is the
  obvious next target. Note that unison does not arise by itself: two pipes with the same
  period keep whatever lag they have (a lag of d moves reproduces itself to within d/128
  per cycle), and the lag only changes when a pipe has two clusters explode together.
- The time-aligned follower works especially well on such a cycle, because the schedule it
  follows repeats.

### Findings on the five ideas

- **"Checkmate" (declare a game lost early).** A capacity bound does this: every split
  costs one unit of capacity (sum of area - 1 over free blocks) and capacity only returns
  when point blocks explode, so fast-forward the board with no splits and stop at the
  first explosion the capacity in hand cannot reach. Along the 2670 game this bound equals
  the moves actually left at every position in the last 70 moves, to the move. So the end
  is decided 70 moves ahead, a new cluster made in that window can never explode in time,
  and the game played out exactly what the bound allowed. That is why ratchet cuts under
  about 100 moves never changed the length. As a search signal it did not help: adding the
  chain left at the end of a rollout to its score changed nothing (paired, -0.8 +/- 3.1
  with plain rollouts), and a play-out policy that picks the move leaving the longest chain
  survives 104 moves from late positions against greedy's 139. Jackson's guess was right
  that it saves no time either: doomed lines already die quickly in rollouts.
- **CPU.** A heuristic rollout move costs about 3 microseconds; there is no large factor
  left there. The gain came from not needing heuristic rollouts: guided-only evaluation
  plus the ending cache make a local-search step roughly ten times cheaper than a ratchet
  attempt, and each step is far more likely to help.
- **A better evaluator learned from the long games.** Three attempts, all null. (1) Tuning
  the heuristic's 14 factors on late-game positions only (move 2000 on, four long games):
  greedy survival 113.8 to 115.4 on the training games and 128.2 to 124 on two held-out
  games, with the five core factors unmoved, the same as session 3's whole-game tuning.
  (2) The chain look-ahead policy above: worse. (3) A look-ahead that refuses moves leading
  to a forced end within 8 moves: +2% (139.3 to 142.3). So greedy play dies in the late
  game for strategic reasons, not from avoidable blunders, and the long games are a better
  guide as schedules to follow (guided rollouts) than as training data for the heuristic.
  Stochastic rollouts remain worth their cost in unguided beams (paired, +29 +/- 6 moves
  for two extra rollouts).
- **One contingent branch?** Partly answered. The old ratchet took a 2073 game with a
  different opening to 2387 in 5 minutes, and the local search (with 30% beam attempts)
  took the same starting pool to 2349 in 64 minutes (`runs/ratchet/lin_std`) before it
  was stopped to give the main line both workers. So other openings climb through the
  2300s the way the main lineage did; whether they reach its level is still open, and
  `95-lineages-later.jobs` continues that run.
- **Delaying the first point blocks.** In 118 from-scratch games the first cluster comes at
  move 5-22 and its timing has no significant rank correlation with length (+0.13; 0.18
  would be significant). Forcing it: with no cluster allowed in the first 30 moves, twelve
  width-100 runs averaged 1370 (baseline 1513, n = 40, sd 234). With no cluster until none
  can be avoided (the first then comes around move 58), two of twelve runs collapsed at
  about 250 and the other ten averaged 1637, best 2086. So a full delay may help when the
  search survives it, and it does not lose anything obvious. `runs/ratchet/lin_nc60`
  (in `95-lineages-later.jobs`) runs the local search on three of those games for 3 hours,
  to see whether the lineage ends up anywhere different. Why it cannot matter much: capacity
  is conserved, so splits made early are not extra, and the late game's difficulty is set
  by the split count itself (a new cluster is worth the split count), not by how long ago
  the first cluster was made.
- **How local the landscape is** (`nbhd`): of about 1,200 single deviations in the last
  500 moves of a long game, 25-37% rejoin and end on exactly the same move, about 1.5% end
  1-9 moves short, the rest end a median of 70-100 moves short, and none ends later. So
  improvements are never one move away; the guided beam finds them by drifting through the
  equal-length rejoining variants, and `ils` by repairing a collapsed kick.
- **Steering a beam towards unison did nothing**: a score penalty on how far neighbouring
  pipes are from identical staircases (`--wsync`, `pipes::unison_dist`) left beam results
  from mid-game cuts unchanged (five paired attempts, +1.6 and -6.0 moves for weights 2 and
  8). The lag between two pipes is not something single moves change.
- **Other paired results**: a to-do-list follower (play the earliest pending guide move
  that is available, instead of the move for the current split count) rejoins 40% more
  often but gave the same kick-and-polish results (+1.1 +/- 2.9); polishing a plain beam
  result adds 14 +/- 6.5 moves but leaves it 59 below its parent on average; kicks 450-1500
  moves from the end never helped (0 of 60); ten polish rounds instead of three made no
  difference; polish width 300 instead of 100 found slightly more at 2.5 times the cost.
- **Emulator risk for a real-game replay.** The long games use a few refill situations
  that the published record games never produced (for example 16 cluster blocks exploding
  into one 4x4 block, at move 552). `refills GAME` lists them with the move where each
  first happens. The paired 4x2 refill the late cycle depends on is not one of them.

## Session 3 (2026-11-06 evening, desktop, Claude Opus 5.5): the ratchet

### What changed

- **`src/rollout_beam.rs`**: the rollout beam moved into the library (rbeam is a thin
  CLI). Same algorithm and the same dedup sort, so session-2 seeds reproduce their games
  exactly (checked: width 30 seed 5 gives 1106 again). New options: `--maxkids M` (at most
  M children of one parent survive selection) and `--rankpen X` (penalty per heuristic rank
  below first, which only matters among rollouts that tie at the cap).
- **`ratchet`**: take the best game, cut it `back` moves before its end, run one beam from
  there with a fresh seed, keep the result if it is at least as long. Repeat for hours. It
  can only improve. v4 keeps a pool of the N longest distinct games and draws each
  attempt's width, rollout cap and stochastic rollouts from lists.
- **`tools/queue`**: a job queue for long runs. Job files in `runs/queue/jobs/*.jobs` run
  in name order (one shell command per line, claimed atomically with mkdir), so an
  experiment file can jump ahead of an overnight file. Workers are started detached with
  PowerShell `Start-Process` so they outlive the session.
- **Tunable heuristic** (`heuristic::HParams`, `tune`): a scale factor on each of the
  heuristic's 14 terms (defaults reproduce Josh Holder's weights exactly; re-checked
  against his Python at all 2261 positions of the 1053 and 1208 games) and a (1+1)
  evolution strategy that maximizes mean greedy survival from strong positions.

### Results

- **The ratchet is what works.** From the 2064 game: 2107, 2138, 2145 in the first five
  minutes, 2238 and 2248 at six minutes; the second leg reached 2265 and 2270, and the
  pool version 2277 in a 46-second test. All verified with `replay`.
- **Where the ratchet's gains come from** (about 140 attempts in the first two legs):
  cuts under 120 moves back *always* tie exactly, so the ending is decided more than 120
  moves before the game ends. Cuts 120 to 480 back produce every improvement. Cuts beyond
  ~500 back cost the most time and never improved anything in these samples (11 of the
  first 35 attempts, 70% of the time). v4 therefore cuts 120-600 back.
- **Seeds from scratch, more samples**: width 100 / k 4 / cap 200 over ten seeds is
  1062-1808, mean 1413, sd 236. The session-2 values 1440 / 1855 / 1885 were lucky draws.
  So the seed lottery alone will not reach the high 2000s; the ratchet is needed.

### Overnight results (2026-11-06 18:00 to 2026-11-07 08:20)

- **Pool ratchet: 2277 to 2620 in 9.5 hours** (`runs/ratchet/pool`, 4,833 attempts, 34
  new bests). The single-game, single-setting ratchet (`r2064b`, width 100, cap 200, no
  stochastic rollouts, ties accepted) went from 2248 to 2332 in 10 hours and 9,919
  attempts. The pool version gained four times as much in less time.
- **Varying the search is what helped.** Inside the pool run, the r2064b setting (w100 cap
  200 stoch 0) produced no gain over its parent in 553 attempts. Attempts with two
  stochastic rollouts were a third of all attempts but produced 24 of the 34 new bests;
  per hour of search, w100 with stoch 2 and cap 200 or 250 gained most often (about 23 gains
  per hour, against 0-10 for stoch 0).
- **Cut depth** (gains over the parent, pool run): 75 in 1,188 attempts cutting 120-179
  back, 12 / 908 at 180-239, 5 / 660 at 240-299, 3 / 533 at 300-359, and 0 / 1,544 at 360
  or more. New bests came from cuts up to 356 back; the two biggest jumps were 2439 to
  2506 (246 back, width 300, cap 250, stoch 2) and 2524 to 2569 (181 back, width 200,
  stoch 2).
- **Then a plateau.** Nothing beat 2620 in the pool run's last 1.6 hours, nor in two more
  4-hour pool ratchets started from it (`morning_a`, `morning_b`, 3,816 attempts). By then
  all eight pool slots held 2620-move games that differed only in their last few dozen
  moves, so every attempt started from the same prefix. Session 4's `--diverse` fixes that.
- **Lineage.** The 2620 game shares its first 1,629 moves with the 2064 game: the ratchet
  rewrote the last 435 moves into 991. The 2248 and 2277 games share 1,932 moves with it,
  and 2439 shares 2,193.
- **The 2620 game** plays the heuristic's first choice at 59.7% of its moves (2064: 60.9%).
  Capacity accounting: 127 + 2,659 from refills - 164 lost to clusters - 2 left over.
- **Beam variants from scratch** (all w100 k4): cap 200, n=40, mean 1513, sd 234, range
  1062-2035; cap 200 with stoch 2, n=20, mean 1612, best 2073; cap 300, n=6, mean 1746,
  range 1490-1976 (the only variant clearly above the baseline, by about 2.5 standard
  errors); noise 0.05, n=6, mean 1447, range 927-2053.

## Session 4 (2026-11-07, desktop, Claude Opus 5.5): breaking the 2620 plateau

- **`ratchet --diverse D`** (v5, snapshot `runs/bin/ratchet_v5.exe`): a result that agrees
  with a pool member up to within D moves of the shorter game's end is in that member's
  family. It replaces the member only if it is longer and is otherwise dropped, so the pool
  holds up to N different families instead of N near-copies. Smoke test from the pool
  run's ten best files: after 8 attempts the pool held 2620, 2614, 2611, 2611, 2610, 2604,
  2599, 2594.
- **Runs queued** (`runs/queue/jobs/60-v5.jobs`, `70-v5-night.jobs`):
  - `v5_pool`: pool 8, diverse 100, slack 40, widths 100/200/300/500, caps 150/250, stochs
    2/2/0/4, cuts 120-400 back, 10 h.
  - `v5_deep`: does a much stronger search rebuild a long tail better? Pool 4, diverse 100,
    slack 60, widths 500/1000, caps 200/400, stoch 2, cuts 200-900 back, 4 h.
  - `v5_night`: like `v5_pool`, seeded with every best file from every ratchet run, 10 h.
- **Beam variants** (from scratch, small samples, all within noise of the baseline):
  `--maxkids 2` 1092-1761 (mean 1507, n=6); `--rankpen 0.3` 1220 / 1846; width 200 with
  `--maxkids 2` 1191 / 1942. Cap 300 and near-noiseless tie-breaks were still running.
- **Tuning the heuristic does not help.** Training survival rose from 201.5 to 202.3 moves
  and held-out survival fell from 208.3 to 201.0; every core term (imbalance, aspect,
  point-block timing, height, next-highest) stayed at its original value. Josh Holder's
  constants sit at a local optimum for greedy survival, so the gains must come from search.
- **Fact check**: Josh Holder's write-up says he took the world record with 1196 (1208 with
  human assistance). It does not mention 1133 or a phone; his repo's commit message says
  "1053 without human aid, 1133 with human aid". Earlier text here saying 1133 was played
  on a phone in January 2022 was an over-reading and has been corrected.

## Session 2 (2026-11-06, desktop, Claude Fable 5.1): rollout beam, 1780 from scratch

Hardware: Ryzen 5 7600 (6 cores / 12 threads), RTX 5070 Ti + RTX 3070 (unused so far).
Rust had to be installed (`winget install Rustlang.Rustup`; cargo is not on the tool shells'
PATH, use `~/.cargo/bin`). The engine runs at 2.7M random-game moves/s per thread here.
Opus's 5× search speedups (previous section) were pulled mid-session; nothing in
`search.rs`/`engine.rs` was touched by this session.

### What changed

The bottleneck was the evaluation, so the evaluation was replaced, not tuned:

1. **Josh Holder's move-scoring heuristic was ported** (`src/heuristic.rs`, from
   `weighting.py` + the parts of `core_update.py` it reads). The port is validated the strong
   way: his Python runs fine here (`python -I` on the clone), so `josh_dump` prints the Rust
   weights in the same format as a Python dump and the two were diffed at **all 4,590
   positions of the four record games: zero mismatches** to 4 decimals. Quirks are kept on
   purpose (e.g. the near-tie "tracker" in ParanoidPlay is reassigned rather than counted, so
   near-tie decision points never fire before move 875; a parabola evaluated at the wrong
   variable; `aboutToLose` can never be true). Greedy play with the heuristic scores **534
   from scratch**, which is exactly the first game in his `logParanoid_final.txt`, at
   **3.4 µs per move** (his Python: several ms).
2. **How much of the record is the heuristic?** The record move is the greedy choice at
   98.9% of positions in the 1053 game, 98.0% in 1133, 94.8% in 1208 (which had human
   edits). The rest are ranks 2-5. So the records are "greedy + a few percent of
   backtracking corrections". But greedy is brittle away from its own trajectory: resumed
   from moves 500 / 700 / 900 / 1100 of the 1208 game it adds only 113 / 73 / 10 / 96 moves.
3. **ParanoidPlay ported** (`src/bin/paranoid.rs`). It reproduces his log nearly game for
   game (685 after 193 games, 748 after 231, 1000 after 652 vs his 657) in 0.1 s; 640k games
   in 60 s reach 1059. Useful as a baseline and for endgame polishing, but it is a DFS over
   ~2% of the moves and plateaus.
4. **The rollout beam** (`src/bin/rbeam.rs`) is the result. Beam search where (a) each node
   expands only its top-k heuristic moves (k = 4), and (b) **each child is scored by how many
   moves a greedy heuristic rollout survives from it** (capped), with the end state's passive
   `survive` feature as a tie-break among rollouts that hit the cap. Every rollout that
   reaches the end of the game is a complete legal game, so the best game is the best
   (depth + rollout) seen anywhere, and it is reconstructed and replayed from scratch before
   being written. Children are deduplicated by position hash; rollouts run in parallel with
   rayon.

### Results (all verified with `replay`, which checks invariants after every move)

| Start | Width / k / cap | Splits | Time (12 threads) |
|---|---|---|---|
| 1208 record game, move 900 (record went on to 1208; old beam 1035-1077) | 100 / 4 / 300 | **1881** | 19 s |
| 1208 record game, move 700 | 100 / 4 / 300 | **1984** | 28 s |
| 1208 record game, move 500 | 100 / 4 / 300 | 1226 | 13 s |
| scratch | 100 / 4 / 300 | 1308 | 30 s |
| scratch | 300 / 4 / 300 | 1352 | 88 s |
| scratch | 100 / 8 / 300 | 1359 | 42 s |
| scratch | 100 / 2 / 300 | 1337 | 17 s |
| scratch | 100 / 4 / 600 | 1508 | 39 s |
| scratch | 100 / 4 / none | 1508 | 40 s |
| scratch | 100 / 4 / 150 | **1780** | 27 s |

The world record is 1277 (by hand). The 1984-split game shows the clockwork at full
stretch: at move 1500 every pipe reads about 1490 / 730 / 350 / 160 / 70 / 30 / 10 from top
to bottom. Both long games end the same way: all 32 slots hold clusters and the last free
blocks are 1 tall when a horizontal split is due.

The rollout-beam games depart from the greedy choice at about a third of their moves
(ranks 2-4), versus 2% for the record games, so the rollout evaluation is doing real work.

### Observations

- **Rollout cap matters and is not monotone** (150 → 1780, 300 → 1308, 600 → 1508). With a
  long cap the beam penalizes positions where greedy fails late, which the beam itself could
  repair; a short cap measures near-term viability and leaves the long-run repair to the
  search. The search is deterministic, so every number above is one chaotic sample; a
  `--seed` option now adds tiny deterministic score noise so a setting can be sampled
  several times. Width 100, k 4, three seeds each, from scratch:

  | Rollout cap | Seeds 1 / 2 / 3 | Mean | Time per run |
  |---|---|---|---|
  | 50 | 1299 / 1267 / 1379 | 1315 | 9 s |
  | 100 | 1217 / 1359 / 1244 | 1273 | 15 s |
  | 150 | 1146 / 1465 / 1471 | 1361 | 21 s |
  | **200** | 1440 / 1855 / **1885** | **1727** | 34 s |
  | 300 | 1654 / 1475 / 1378 | 1502 | 33 s |
  | 150, `--policy both` | 1391 / 1430 / 1305 | 1375 | 51 s |

  Cap 200 is the setting to use; the seed-to-seed spread (1440 to 1885) means many seeds
  should be run and the best kept. `--policy both` buys nothing for twice the cost.

  Width 300 at cap 200, thirteen seeds: 1692 / 1586 / **2039** / 1474 / 1648 / 1627 /
  1710 / 1207 / 1747 / 1022 / 1170 / 1269 / 1332 (mean 1502, about 100 s each). So a wider
  beam does not lift the typical game; the best game is set by the tail of the
  distribution, and the cheapest way to a better best is more seeds. Width 1000 runs are
  in `runs/exp/rbeam_long2.log` if they finished after this was written.
- **Why greedy rollouts die**: from strong positions (moves 600 / 1000 / 1400 of the 1780
  game) greedy adds 255 / 154 / 183 moves and the dead board always shows "twins": adjacent
  clusters in one pipe with nearly equal values (853 / 845 / 847, or 368 / 368 / 358 / 351 /
  354). A cluster placed on a pipe before the one beneath it has been halved breaks the
  geometric chain and wastes a slot. `src/pipes.rs` has a `twins` count (`--wtwin` in
  rbeam). As a static feature it is only weakly separating (2-8 twins on live boards, 1-6
  on dead ones), and **as a penalty it is harmful**: `--wtwin 3` gave 1389 / 1013 / 915 and
  `--wtwin 1` gave 970 / 812 / 834 (3 seeds, cap 200) against 1440 / 1855 / 1885 without it.
  Twins near the top of a pipe are often the right move (the halving-timing trick stacks
  two clusters that will be halved together).
- **A second failure mode: slow poison.** A weak run (1215 at cap 200 with two stochastic
  rollouts, seed 2) had, by move 1100, a 572-valued cluster at the very bottom of a pipe with
  nothing beneath it, and that pipe fragmented into 1-wide columns with a permanent
  one-column void. A 200-move rollout cannot see the end of that. Session 1's passive
  look-ahead features do: `stuck` (point area still present after 250 passive moves) is
  58-68 for that game at moves 900-1100 versus 8-24 for the good games, and `debt` 190-222
  versus 65-108. Both are now rbeam options (`--wstuck`, `--wdebt`); results below.
- **Two extra stochastic rollouts per child** (`--stoch 2 --temp 3`, keep the longest of
  three): 2064 / 1215 / 1511 over three seeds at cap 200, three times the cost. Not
  consistently better than one greedy rollout, but it produced the best game.
- **Static penalties on the slow-poison features are harmful too**: `--wstuck 0.5` gave
  1436 / 1033 / 859, `--wstuck 2` 1126 / 1055 / 906, `--wdebt 0.1` 1273 / 783 / 646.
  Every static term added to the rollout length has made the beam worse. The rollout
  length is a better judge of a position than any feature computed from it, and the right
  fix for its blind spots is probably a better rollout policy, not a correction term.

### The game, in numbers (`stats` export of the 2064 game)

`src/bin/stats.rs` prints per-move statistics (cluster values per pipe, free cells,
legal moves, greedy rollout length, the rollout lengths of the top-4 alternatives and the
rank the game took) and `--json` writes them for the visualization. Per 100-move window of
the 2064 game:

| moves | clusters / pipe | free cells | legal moves | greedy rollout | took rank 1 | median ratio of adjacent clusters |
|---|---|---|---|---|---|---|
| 0-100 | 1.6 | 91 | 8.9 | 324 | 43% | - |
| 100-300 | 3.2-4.0 | 48-58 | 9-11 | 290-300 | 63% | 1.0 |
| 300-600 | 4.6-5.8 | 34-40 | 7-8 | 250-260 | 58-63% | 1.2-2.1 |
| 600-900 | 5.8-6.6 | 18-33 | 4-7 | 210-260 | 48-64% | 1.1-2.1 |
| 900-1800 | 6.3-6.8 | 16-28 | 3.4-5 | 190-245 | 53-68% | 2.1 |
| 1800-2064 | 6.8-6.9 | 15-18 | 3.4-4.2 | 150 → 32 | 66-81% | 2.1 |

Every pipe holds two clusters by move 82, three by 176, five by 346 and seven by 621. One
cluster is created every 3.17 moves; 626 of the 651 clusters are 2×2 groups of 1×1 blocks
(12 are 1×2, 9 are 2×1, 2 are 2×2, 2 are 8-block groups). 354 cluster-creating moves are
vertical splits and 297 horizontal. The game's move is the heuristic's first choice 1256
times, second 452, third 229, fourth 127; at 321 moves the move taken was not even the
longest-rollout alternative, so the beam's look-ahead beyond one ply matters.

Exact capacity accounting from the engine (`MoveInfo.refill_cap`, `new_point_cap`):
2064 = 127 + 2094 (capacity of refill blocks) − 156 (capacity lost when non-1×1 blocks
became point blocks) − 1 left over. With one 2×2 refill per cluster the refill term would
be 3 × 651 = 1953; the extra 141 came from bigger refills (a 2×4 when two stacked clusters
explode on the same move, a 4×2 when neighbouring pipes fire together). Josh Holder's 1208
game: 1208 = 127 + 1323 − 242.

### Upper bounds (how long can a game possibly be?)

**Capacity is exactly conserved per cycle.** Define K = Σ over free blocks of (area − 1).
Every split costs exactly 1 of K. When four blocks of area a become point blocks, K
drops by 4(a − 1). An explosion freeing cells and a refill block of area R adds R − 1. So
moves = 127 + Σ(R − 1) − Σ 4(a − 1) − K_final, exactly (verified above). A 2×2 refill
turned into a 2×2 cluster of 1×1 blocks: +3, −3 splits, −0 lost: the cycle is neutral and
every explosion funds exactly 3 moves. A bigger refill covering c clusters' worth of cells
funds 4c − 1 moves, i.e. under 4 per cluster. **So the game needs one cluster explosion
every 3 moves (at most every 4 with the biggest conceivable refills), forever.**

**Halvings are capped at 7.** A cluster's two blocks in a column are stacked and explode
together, so each explosion beneath a block removes at least two rows; sixteen rows
minus the block's own two leave room for seven explosions beneath it. Pipes are at least
two columns wide (a cluster is a 2×2 arrangement), so there are at most four of them.

**The chain.** A cluster enters a pipe worth the move number n, counts down 1 per move
and is halved (floor) at each explosion beneath it. With a pipe of depth 8 (7 halvings)
firing every I moves, the value reaching the bottom is about n/128 − I, and in steady
state that must equal I, so I ≈ n/256. A single-pipe simulation (clusters created three
moves after each explosion, bunched explosions queued) gives **I ≈ n/265 for depth 8,
stable to ±1, and n/133 for depth 7**. Four pipes must deliver one explosion per 3 moves,
so I ≤ 12 with plain 2×2 refills, ≤ 12.9 with the refill mix the 2064 game achieved (3.22
moves per cluster), ≤ 15 with every refill an 8×2 (3.75 per cluster), and ≤ 16 is the
absolute limit. The clockwork therefore stops keeping up at about **n = 3,200 (plain) /
3,400 (observed refill mix) / 4,000 (perfect 8×2 refills) / 4,250 (absolute)**.

**The tail.** Past that point the interval I = n/265 keeps growing and the deficit per move
is 1 − n0/n, which integrates to (n − n0) − n0·ln(n/n0). With K0 ≈ 25 moves of slack on the
board that is exhausted at n ≈ n0 (1 + √(2K0/n0)), about 12% past n0: roughly **3,550 /
3,800 / 4,500 / 4,750**. So the best hard-ish upper bound I can defend is about **4,700
splits**, and the ceiling for a perfectly timed four-pipe clockwork with realistic refills
is about **3,800**.

(Session 7: the 3898 game is such a clockwork, with two pairs: 14 splits back per period,
a balance point of 3,710, and a reserve of about 7 splits instead of 25. The estimate
below was too low; the reasons given for it describe the rollout searches, not the game.)

**What I think the true maximum is: about 3,000, likely between 2,700 and 3,400.** The
reasons it is below the 3,800 clockwork ceiling: (1) depth 8 means a pipe is entirely
point blocks between its refill and its next explosion, so the four pipes must be
staggered within a move or two of perfectly, with integer floors in every halving; any
slack the player keeps for timing errors is a cluster not in a pipe, and a depth-7 pipe
tops out around 1,700; (2) the opening spends the first 600 moves building depth, and the
clusters created then enter small pipes; (3) direction alternation forces the four pipes
into interleaved VHH / HVV patterns, which fights the staggering; (4) the 2064 game
already runs mostly depth-8 pipes with a median adjacent ratio of 2.1 and dies when the
margin collapses, so the remaining gain must come from near-perfect staggering, and the
rollout beam has no model of that. The reasons it is above 2,064: thirteen width-300
seeds landed between 1,022 and 2,039, so the search is nowhere near the structure's
limit; and the clockwork at n = 2,064 still has slack (depth 8 needs I = 7.8, the pipes
can afford 12.9).
- The "avoid big clusters" rollout policy (take the best move that does not create point
  blocks bigger than 1×1) differs from greedy at 18 of 534 moves of the greedy game and
  scores 435 from scratch, so it is not simply better; `--policy both` takes the longer of
  the two rollouts at double cost. `--stoch N --temp T` adds N softmax-sampled rollouts per
  child (seeded, replayable) and keeps the longest.

## What I believe about the game (theory)

**The pipeline picture.** The record games settle into four 2-column pipes. Each is a stack
of 2×2 clusters of 1×1 blocks, with values roughly halving each step down. At record move
1000, one pipe reads 1000 / 496 / 243 / 115 / 52 / 21 / 6 from top to bottom. When the
bottom cluster explodes, everything above falls one slot and is halved, and a fresh 2×2
refills the top. Splitting that refill takes 3 moves (H,V,V or V,H,H), and the result
becomes the new top cluster, valued at the current move number.

**Throughput bound (back of the envelope).** Each explosion gives 3 splits, so the four
pipes together need one explosion every ~3 moves, i.e. about one every 12 moves per pipe.
A fresh cluster with value `n` on top of a pipe with k clusters beneath it reaches the
bottom with value ≈ n/2^k − 12·(1 − 2^−k). It needs to be ≤ ~12 there.

- k = 6: n ≲ 1500.
- k = 7 (all 8 slots used, no spare room): n ≲ 3000. That matches Josh Holder's "theoretical
  ~3700, practical ~1800".

The hard parts are keeping all four pipes synchronized, balancing H and V (pipes must
interleave HVV and VHH patterns), and avoiding accidental clusters between neighbouring
pipes' half-split blocks.

**Catastrophes to avoid:**

1. A fresh cluster ending up at the bottom of a pipe, or anywhere with nothing beneath it
   due to explode. It's dead weight for ~n moves. A common way in: free blocks inside a pipe
   fall into a gap and form a cluster with value `n`.
2. Clusters of big blocks (2×1, 2×2, ...), which lock area and capacity.
3. 6+ block clusters.
4. Direction starvation: all free blocks thin in the needed direction.
5. Wide point blocks spanning pipes.

**Josh Holder's halving-timing trick.** Create a cluster just before an explosion beneath
it, not just after, to gain a halving. Halving floors, which makes the exact condition
fiddly; his `doesPointBlockCauseNewHalving()` in `weighting_new.py` works it out.

## Why I think the beam search fails

- Every evaluation term is computed from a **passive** look-ahead of existing blocks.
  Future clusters' fates depend on decisions the evaluation can't see. The `slot` and
  `pipeline_model` terms try to approximate that and help a little.
- The damage from bad structure (dead weight at the bottom) shows up hundreds of moves
  later, past the horizon. By then every state in the beam shares it, because beams collapse
  onto a common ancestor quickly. More width doesn't fix a *systematic* bias.
- Weights were hand-tuned from single noisy runs. A one-parameter change sends the game
  down an entirely different path, so a single run says little; use the prefix benchmark
  instead.

## Tools added in session 2

- `src/bin/stats.rs` (per-move statistics, `--rollout`, `--alts`, `--json`), used by the
  "SPL-T Clockwork" visualization (an HTML page built from `runs/best/scratch_2064_stats.json`,
  the run list in `runs/exp`, and a `rbeam --tree` dump of a width-30 run).
- `rbeam --tree FILE` dumps the beam genealogy (parent index and rollout length of every
  kept node per level).
- `src/bin/export_game.rs` + `tools/replay.html` (built in a parallel session): per-move
  boards as JSON and a page that highlights the block to tap, for replaying a sequence on
  the real game.
- `docs/OUTREACH.md` (git-ignored, holds contact details) lists people who might want to
  hear about this.

## Suggested next steps (roughly in order of expected value)

1. **Fit the evaluation to expert moves.** For each of the ~1200 positions in the record
   game(s), treat the record move as the "label" among its siblings. Fit feature weights by
   conditional logit / learning-to-rank (like Texel tuning in chess). The `features()`
   vector already exists. Add per-pipe features: values from bottom to top, the halving
   ratio between neighbours, free cells per pipe, pipe height, and the lowest free block.
2. **Port Josh Holder's heuristic plus ParanoidPlay backtracking** (`weighting.py`,
   `randomParanoidPlay.py`) onto the Rust engine. In Python it reached 1196 in about 36 hours
   and 700k games; in Rust that is perhaps a 100× speedup. It could also serve as a strong
   rollout policy or move filter for the beam.
3. **Make the pipeline model direction- and occupancy-aware.** It currently ignores H/V
   alternation, pipe height limits (8 slots), odd shapes, and accidental cross-pipe
   clusters. Calibrate its predicted death time against actual outcomes on beam games.
4. **Search-side fixes**: cap children per parent for diversity; use the prefix benchmark
   (500/700/900) for every evaluation change; try NRPA or MCTS with rollouts.
5. **GPUs**: once a decent policy exists, generate self-play data and train a small CNN
   value/policy net on the 8×16 grid (channels: free w, h, point value / n, and so on).
   Josh's `nn_split` tried AlphaZero from scratch and found it far too slow. Bootstrapping
   from a strong heuristic is the fix he suggested.
6. **Endgame polishing**: re-searching the last ~100 moves of strong games already gained
   +1 on 1208. Cheap and worth automating.
7. **Human replay tool**: an HTML page that steps through a sequence and highlights the next
   block to tap. Needed for any real record attempt, since sequences are block-index lists
   in engine order.

## Practical notes

- Build: `cargo build --release` (Rust 1.99, edition 2024; the only dependency is rayon).
  The release profile uses fat LTO.
- `reference/` is git-ignored and must be cloned:
  `git clone https://github.com/josh-holder/SPLT reference/josh-holder-SPLT`. That repo
  has no license file, so its code and sequences are not redistributed here.
- Useful commands:
  `target/release/replay <seq> [--show N | --final]`,
  `target/release/trace <seq> 50`,
  `target/release/solve --width 200 --prefix <seq> --keep 900 --quiet`.
- On this Windows laptop, the Bash and PowerShell tools see different `%LOCALAPPDATA%`.
  Cargo lives in `~/.cargo`, so it works from either, but pick one shell per session.
- Engine invariants: `Board::check_invariants()` (no overlaps, grid in sync, no floating
  blocks). `replay` calls it after every move.

## Sources

- Josh Holder, "Breaking the SPL-T World Record" (2022): https://josh-holder.github.io/projects/splt-blog
- Code and record sequences: https://github.com/josh-holder/SPLT ; RL attempt: https://github.com/josh-holder/nn_split
- Craig Polley's brute-force post: http://www.flashingleds.net/brute_SPL-T/brute_spl-t.html
- Record history plot: https://bsky.app/profile/ryederek.bsky.social/post/3m7dvhfghj22h
- Strategy guides: https://www.levelwinner.com/spl-t-cheats-tips-strategy-guide-for-solving-more-puzzles/ ,
  https://www.appspy.com/features/hidden-gem-of-the-week-spl-t/
- Simogo's page: https://simogo.com/work/spl-t/
