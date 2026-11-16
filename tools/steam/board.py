"""The SPL-T board as it appears in the Steam version (Simogo Legacy Collection), and the
record game's positions from the Rust engine.

Screen geometry. SPL-T's screen keeps the iPad's proportions at any window size: the
board is 8 cells wide and 31/24 of its width tall (16 cells), with the score bar, 1/24 of
the width tall, above it, so the whole screen is 3:4. The score bar ends in a dashed
one-pixel line whose ends are the board's left and right edges; `find_geom` locates the
board from that line alone, so it works before the first move. At 3840 x 2160 fullscreen
(measured 2026-11-15) the board is x 1152 to 2688, y 120 to 2104: cells of 192 x 124.
Block edges are one-pixel white lines; point blocks are hatched with a number in each
cell; empty cells are white.
"""
import json
import os
import re
import subprocess

import numpy as np

W, H = 8, 16
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
CALIB = os.path.join(os.path.dirname(__file__), 'calib.json')


class Geom:
    """Where the board is on screen: top-left corner (x0, y0) and cell size, in pixels."""

    def __init__(self, x0, y0, cw, ch):
        self.x0, self.y0, self.cw, self.ch = float(x0), float(y0), float(cw), float(ch)

    @property
    def region(self):
        """The board as an integer screen rectangle (left, top, right, bottom)."""
        return (int(round(self.x0)), int(round(self.y0)),
                int(round(self.x0 + W * self.cw)), int(round(self.y0 + H * self.ch)))

    @property
    def screen(self):
        """SPL-T's whole screen (score bar and board, 3:4) as (left, top, width, height),
        for cropping a recording. Width and height are rounded to even numbers."""
        w = W * self.cw
        top = self.y0 - w / 24
        wi = int(round(w)) // 2 * 2
        return (int(round(self.x0)), int(round(top)), wi, int(round(w * 4 / 3)) // 2 * 2)

    @property
    def scale(self):
        """Size relative to the 3840 x 2160 fullscreen layout."""
        return self.cw / 192.0

    def cell_px(self, cx, cy):
        return self.x0 + self.cw * (cx + 0.5), self.y0 + self.ch * (cy + 0.5)

    def block_px(self, b):
        """Screen pixel of the centre of a block (x, y, w, h, pts)."""
        x, y, w, h = b[:4]
        return self.x0 + self.cw * (x + w / 2), self.y0 + self.ch * (y + h / 2)

    def to_dict(self):
        return {'x0': self.x0, 'y0': self.y0, 'cw': self.cw, 'ch': self.ch}


GEOM_4K = Geom(1152, 120, 192, 124)


def _dashes(row):
    """The longest stretch of a boolean row made of short alternating lit and dark runs
    (a dashed line): (first lit x, last lit x, typical dark run) or None."""
    xs = np.flatnonzero(np.diff(np.concatenate(([0], row.astype(np.int8), [0]))))
    if len(xs) < 4:
        return None
    starts, ends = xs[0::2], xs[1::2]  # lit runs [start, end)
    best, i = None, 0
    while i < len(starts):
        j = i
        while (j + 1 < len(starts) and ends[j] - starts[j] <= 6 and starts[j + 1] - ends[j] <= 6):
            j += 1
        if j - i >= 20 and ends[j] - starts[j] <= 6:
            gaps = starts[i + 1:j + 1] - ends[i:j]
            cand = (int(starts[i]), int(ends[j] - 1), int(np.median(gaps)))
            if best is None or cand[1] - cand[0] > best[1] - best[0]:
                best = cand
        i = j + 1
    return best


def find_geom(img, client=None):
    """Locate the board in a screenshot of SPL-T's game screen (any position, including
    0 splits) from the score bar's dashed line (one pixel tall, two pixels on and two off,
    at any window size). `img`: a PIL image of the whole screen; `client`: the game
    window's client rectangle, to limit the search. Returns a Geom or None."""
    a = np.asarray(img.convert('L'), dtype=np.int16)
    l, t, r, b = client or (0, 0, a.shape[1], a.shape[0])
    sub = a[t:b, l:r] > 150
    best = None
    for y in range(sub.shape[0] // 3):
        d = _dashes(sub[y])
        if d and d[1] - d[0] > 0.25 * (r - l) and (best is None or d[1] - d[0] > best[2] - best[1]):
            best = (y, d[0], d[1], d[2])
    if best is None:
        return None
    y, xa, xb, gap = best
    width = xb - xa + 1 + gap  # the line ends with a dark gap at the board's right edge
    return Geom(l + xa, t + y + 1, width / W, width * 31 / 24 / H)


def save_calib(geom, cursor):
    with open(CALIB, 'w') as f:
        json.dump({'board': geom.to_dict(), 'cursor': cursor}, f, indent=1)


def load_calib():
    if not os.path.exists(CALIB):
        return None
    with open(CALIB) as f:
        return json.load(f)


def positions(game):
    """Every position of a game, from the engine: positions[k] is the list of blocks
    (x, y, w, h, pts) in engine order after k moves. Uses src/bin/frames.rs."""
    exe = None
    for d in ('target/dev4/release', 'target/release'):
        p = os.path.join(REPO, d, 'frames.exe')
        if os.path.exists(p):
            exe = p
            break
    if exe is None:
        subprocess.run(['cargo', 'build', '--release', '--bin', 'frames'], cwd=REPO, check=True)
        exe = os.path.join(REPO, 'target/release/frames.exe')
    out = subprocess.run([exe, game], cwd=REPO, capture_output=True, text=True, check=True).stdout
    pos = []
    for line in out.splitlines():
        if not line.strip():
            continue
        pos.append([tuple(int(v) for v in b.split()) for b in line.split(';')])
    return pos


def moves(game):
    return [int(m) for m in re.findall(r'\d+', open(os.path.join(REPO, game)).read())]


def cell_map(blocks):
    """grid[cy][cx] = index of the block covering the cell, or -1 for a void."""
    grid = [[-1] * W for _ in range(H)]
    for i, (x, y, w, h, pts) in enumerate(blocks):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                grid[yy][xx] = i
    return grid
