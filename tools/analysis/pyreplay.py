"""Replay a sequence through Josh Holder's Python emulator (core_update.py) and report how
many moves it accepts and whether the game is over afterwards. This is the strongest check
available short of the real game: the Rust engine is a port of that emulator, and this runs
the original.

usage: python tools/analysis/pyreplay.py REFERENCE_DIR SEQ_FILE
  REFERENCE_DIR  a checkout of github.com/josh-holder/SPLT (the folder holding core_update.py)
"""
import os
import re
import sys

ref, seq = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
moves = [int(x) for x in re.split(r'[^0-9]+', open(seq).read()) if x]
sys.path.insert(0, ref)
os.chdir(ref)
import core_update as core  # noqa: E402

gb = core.Board()
for k, m in enumerate(moves):
    if m >= len(gb.box) or not gb.box[m].splitPossible(gb.splitAction):
        print('the reference emulator rejects move %d (box %d)' % (k + 1, m))
        break
    core.makeMove(gb, m, False)
left = len(gb.getMoveOptions())
print('%s: %d of %d moves accepted by the Python reference; legal moves left: %d%s' % (
    os.path.basename(seq), len(gb.splitRecord), len(moves), left,
    ' (game over - matches)' if left == 0 and len(gb.splitRecord) == len(moves) else ''))
