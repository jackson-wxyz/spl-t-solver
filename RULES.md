# SPL-T rules (as implemented in `src/engine.rs`)

These rules are a port of the Python emulator `core.py` in
[josh-holder/SPLT](https://github.com/josh-holder/SPLT), which descends from Craig Polley's
[flashingleds.net emulator](http://www.flashingleds.net/brute_SPL-T/brute_spl-t.html).
Move sequences produced with that emulator were replayed by hand on a real iPhone to set the
1133-split world record (Josh Holder, Jan 2022). So it has been checked against the real game
over at least 1133 consecutive moves.

**Validation of this port.** `cargo run --release --bin replay <file>` replays each of the
four record sequences in `reference/josh-holder-SPLT/sequences/` (1053, 1133, 1196, 1208
splits). Every move is legal, and each game ends with zero legal moves on exactly its
last move. A rules mismatch would almost certainly produce an illegal move or a
different game length within a few hundred moves.

## Board

- 8 columns × 16 rows. Coordinates: `x` left→right, `y` **top→bottom** (row 0 is the top).
- The game starts as a single 8×16 block.
- Every block has power-of-two width and height.

## A move

You tap a block to split it in half. The **direction is forced and strictly alternates**:
the first split is horizontal (top/bottom halves), the next is vertical (left/right),
then horizontal again, and so on. A block can be split only if it is not a point block
and is at least 2 long in the split direction. **The game ends when no block can be split in
the current direction.** Your score is the number of splits made.

After the split, these steps run in order (`n` = splits made so far, including this one):

1. **Split.** The block shrinks in place to its top/left half. The bottom/right half
   becomes a new block.
2. **Clusters from the split.** Any 2×2 arrangement of four *identical* (same w×h),
   exactly aligned, non-point blocks that includes one of the two halves becomes a
   cluster. All its members become **point blocks with value n+1**; step 3 brings that
   down to `n`. Overlapping groups all convert, so a 2×3 run of six identical blocks
   becomes six point blocks.
3. **Countdown.** Every point block loses 1. A block at 1 **explodes** instead.
4. **Explosions.** Exploded blocks are removed.
5. **Gravity.** Repeatedly, any block whose whole bottom edge sits over empty cells falls
   as far as it can, until nothing moves. This also runs when nothing exploded: a vertical
   split of a block that bridges a hole can leave one half unsupported. A block that rests
   on at least one column stays put even if other columns under it are empty, so **holes
   can persist**.
6. **Refill from the top.** The emulator's rules here are quirky but they match the real
   game:
   - If something fell, count the empty cells from the top of every column. If nothing fell
     but something exploded, count only empty cells in rows where an explosion happened. In
     that case older voids are not refilled until something falls through them.
   - Repeatedly take the leftmost "pocket" (a run of adjacent columns that have voids). A
     one-column pocket is never filled. Otherwise find the deepest part of the pocket and
     round its width down to a power of two. If the depth is odd, give up on its top layer.
     Otherwise drop in one block that wide and as tall as the largest power of two that
     fits. Repeat until no voids are counted.
7. **Clusters from falling.** If anything fell, every 2×2 group of identical free blocks
   anywhere on the board becomes point blocks with value `n`.
8. **Halving.** Every point block that fell this move has its value halved, rounding down
   (5 → 2, 2 → 1; 1 stays 1). A block is halved once per move however far it fell.

## Consequences that matter for strategy

- A new cluster's value equals the move number, so values grow as the game goes on. A
  cluster made at move 1000 sitting at the bottom of the board takes 1000 moves to clear.
- The only way to shrink a big value is halving, which needs something **beneath** it to
  explode. A cluster on top of a stack of k clusters that explode one after another gets
  halved k times.
- The record games settle into four 2-column "pipes", each a stack of 2×2 clusters of
  1×1 blocks, with values roughly halving at each step down (e.g. 1000 / 496 / 243 / 115 /
  52 / 21 / 6). Each bottom explosion refills the top of its pipe with a fresh 2×2 block,
  which takes 3 splits (HVV or VHH) to turn into the next cluster on top.
- Bigger clusters (of 2×1, 2×2, ... blocks) lock up more area and splitting capacity. Free
  blocks that fall into a gap can form a cluster that then sits at the bottom with a huge
  value.

## Open questions / unverified edge cases

- The two emulator versions differ in one corner case. `core.py` checks clusters of the new
  block's size anywhere on the board after a split. `core_update.py`, used to generate the
  record sequences, only checks groups that include a half of the split block. This port
  follows `core_update.py`. They can differ only after a refill that happens without
  anything falling. The record sequences never hit a difference.
- Whether the 2025 Simogo Legacy Collection (Steam/Switch) version uses identical rules is
  not verified. It very likely does.
