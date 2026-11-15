"""The SPL-T board as it appears in the Steam version (Simogo Legacy Collection), and the
record game's positions from the Rust engine.

Screen geometry, measured on 2026-11-15 with the game fullscreen at 3840 x 2160: the board
is the tablet's screen, x 1152 to 2688 and y 120 to 2104, so a cell is 192 x 124 pixels.
Block edges are drawn as one-pixel white lines on the top or left edge of the lower or
right block (the first split's line is at y = 120 + 8 x 124 = 1112).
"""
import os
import subprocess

X0, Y0, CW, CH = 1152, 120, 192, 124
W, H = 8, 16
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))


def cell_px(cx, cy):
    """Screen pixel of the centre of cell (cx, cy)."""
    return X0 + CW * (cx + 0.5), Y0 + CH * (cy + 0.5)


def block_px(b):
    """Screen pixel of the centre of a block (x, y, w, h, pts)."""
    x, y, w, h = b[:4]
    return X0 + CW * (x + w / 2), Y0 + CH * (y + h / 2)


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
    import re
    return [int(m) for m in re.findall(r'\d+', open(os.path.join(REPO, game)).read())]


def cell_map(blocks):
    """grid[cy][cx] = index of the block covering the cell, or -1 for a void."""
    grid = [[-1] * W for _ in range(H)]
    for i, (x, y, w, h, pts) in enumerate(blocks):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                grid[yy][xx] = i
    return grid
