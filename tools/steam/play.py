"""Play a solver game on the Steam version of SPL-T (Simogo Legacy Collection), checking
the screen against the engine after every move.

usage: python play.py [--game runs/best/game_3898.txt] --from K [--to N]
                      [--gap 0.10] [--log FILE] [--shots DIR] [--record FILE.mp4]

Run it from PowerShell, not from Git Bash inside the Claude desktop app: screen capture
(Desktop Duplication, used by --record) fails from that sandbox.

The game must be open at the position after K - 1 moves (for a new game: K = 1 and the
board one black block). Move k clicks the centre of the block the game splits at move k,
waits, takes a screenshot and compares it with the engine's position after move k: which
cells are free, point or empty, and where the edges between free blocks are. A move that
explodes a cluster waits for the board to stop changing first (about 0.9 s). On a
mismatch it retries for a while (an animation may still be running), then stops and
saves the screenshot.

--record: capture the whole screen with ffmpeg (Desktop Duplication, NVIDIA encoder, native
4K at 30 fps) from just before the first move to just after the last; `video.py` makes the
1080p copy afterwards.

The run stops if the game is no longer the foreground window (the clicks would otherwise
land in whatever window is in front). Do not touch the mouse while it runs: the game's cursor is steered by relative mouse
motion, and a nudge from a real mouse puts every later click in the wrong place.
"""
import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time

import numpy as np

import board
import desk

X0, Y0, CW, CH, W, H = board.X0, board.Y0, board.CW, board.CH, board.W, board.H
REGION = (X0, Y0, X0 + CW * W, Y0 + CH * H)
FREE, POINT, VOID = 0, 1, 2


def screen_board():
    """The board region as a grayscale array (board pixels, origin at the board's corner)."""
    return np.asarray(desk.grab(REGION).convert('L'), dtype=np.uint8)


def expected(blocks):
    """(type per cell, block index per cell) for an engine position."""
    grid = board.cell_map(blocks)
    typ = np.full((H, W), VOID, dtype=np.int8)
    for cy in range(H):
        for cx in range(W):
            i = grid[cy][cx]
            if i >= 0:
                typ[cy, cx] = POINT if blocks[i][4] > 0 else FREE
    return typ, np.array(grid)


def cell_type(g, cx, cy):
    x0, x1 = int(cx * CW + CW * 0.15), int(cx * CW + CW * 0.85)
    y0, y1 = int(cy * CH + CH * 0.2), int(cy * CH + CH * 0.8)
    f = (g[y0:y1, x0:x1] > 100).mean()
    return (FREE if f < 0.01 else VOID if f > 0.9 else POINT), f


def vline(g, cx, cy):
    """Is there an edge line between cell (cx, cy) and (cx + 1, cy)?"""
    x = (cx + 1) * CW
    y0, y1 = int(cy * CH + CH * 0.25), int(cy * CH + CH * 0.75)
    strip = g[y0:y1, x - 2:x + 2].max(axis=1)
    return (strip > 100).mean() > 0.8


def hline(g, cx, cy):
    """Is there an edge line between cell (cx, cy) and (cx, cy + 1)?"""
    y = (cy + 1) * CH
    x0, x1 = int(cx * CW + CW * 0.25), int(cx * CW + CW * 0.75)
    strip = g[y - 2:y + 2, x0:x1].max(axis=0)
    return (strip > 100).mean() > 0.8


def compare(g, blocks, hide=None):
    """Differences between the screen and an engine position, as a list of strings.
    `hide`: a rectangle in board pixels (the hand cursor) where nothing is checked."""
    typ, grid = expected(blocks)

    def hidden(x0, y0, x1, y1):
        return hide is not None and x0 < hide[2] and x1 > hide[0] and y0 < hide[3] and y1 > hide[1]

    bad = []
    for cy in range(H):
        for cx in range(W):
            if hidden(cx * CW, cy * CH, (cx + 1) * CW, (cy + 1) * CH):
                continue
            t, f = cell_type(g, cx, cy)
            if t != typ[cy, cx]:
                bad.append(f'cell ({cx},{cy}) looks {"FPV"[t]} ({f:.2f}), expected {"FPV"[typ[cy, cx]]}')
    for cy in range(H):
        for cx in range(W - 1):
            if typ[cy, cx] == FREE and typ[cy, cx + 1] == FREE and not hidden((cx + 1) * CW - 4, cy * CH, (cx + 1) * CW + 4, (cy + 1) * CH):
                want = grid[cy, cx] != grid[cy, cx + 1]
                if vline(g, cx, cy) != want:
                    bad.append(f'edge right of ({cx},{cy}): {"missing" if want else "unexpected"}')
    for cy in range(H - 1):
        for cx in range(W):
            if typ[cy, cx] == FREE and typ[cy + 1, cx] == FREE and not hidden(cx * CW, (cy + 1) * CH - 4, (cx + 1) * CW, (cy + 1) * CH + 4):
                want = grid[cy, cx] != grid[cy + 1, cx]
                if hline(g, cx, cy) != want:
                    bad.append(f'edge below ({cx},{cy}): {"missing" if want else "unexpected"}')
    return bad


def animations(pos):
    """For every move, whether the board animates after it: a cluster explodes (some point
    block is worth 1 before the move), or blocks fall without an explosion (a vertical split
    that leaves half a block over a hole; 7 moves in the 3898 game). The second case is found
    by comparing the free cells below each block before and after."""
    out = []
    for k in range(1, len(pos)):
        before, after = pos[k - 1], pos[k]
        boom = any(b[4] == 1 for b in before)
        if not boom:
            # A fall moves a block down: some block of `after` (other than the two halves of
            # the split) is not in `before` with the same geometry and value minus one.
            prev = {(x, y, w, h) for (x, y, w, h, p) in before}
            new = [b for b in after if (b[0], b[1], b[2], b[3]) not in prev]
            boom = len(new) > 2
        out.append(boom)
    return out


def ffmpeg():
    """Path of ffmpeg (on PATH, or where winget puts it)."""
    return shutil.which('ffmpeg') or os.path.join(os.environ.get('LOCALAPPDATA', ''), 'Microsoft', 'WinGet', 'Links', 'ffmpeg.exe')


def start_recording(path):
    """ffmpeg capturing the screen (Desktop Duplication) to an H.264 file, encoded on the
    GPU at the screen's own resolution. Returns the process; `stop_recording` ends it."""
    ff = shutil.which('ffmpeg') or os.path.expandvars(r'%LOCALAPPDATA%\Microsoft\WinGet\Linksfmpeg.exe')
    cmd = [ff, '-y', '-hide_banner', '-loglevel', 'warning',
           '-f', 'lavfi', '-i', 'ddagrab=output_idx=0:framerate=30:draw_mouse=0',
           '-c:v', 'h264_nvenc', '-preset', 'p5', '-rc', 'vbr', '-cq', '18', '-b:v', '0', '-g', '60',
           path]
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    time.sleep(1.5)  # ffmpeg needs a moment before the first frame
    if p.poll() is not None:
        raise RuntimeError('ffmpeg did not start: ' + p.stderr.read().decode(errors='replace'))
    return p


def stop_recording(p):
    try:
        p.stdin.write(b'q')
        p.stdin.flush()
    except OSError:
        pass
    try:
        p.wait(timeout=20)
    except subprocess.TimeoutExpired:
        p.kill()


def hand_box(cur):
    """The hand sprite's rectangle in board pixels (generous)."""
    px, py = cur.ux * cur.GAIN - X0, cur.uy * cur.GAIN - Y0
    return (px - 20, py - 15, px + 100, py + 125)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game', default='runs/best/game_3898.txt')
    ap.add_argument('--from', dest='start', type=int, required=True)
    ap.add_argument('--to', type=int, default=None)
    ap.add_argument('--gap', type=float, default=0.10, help='least time between two clicks (s)')
    ap.add_argument('--settle', type=float, default=0.45, help='wait before checking an explosion move (s)')
    ap.add_argument('--log', default=None)
    ap.add_argument('--shots', default=None, help='folder for a screenshot on failure')
    ap.add_argument('--record', default=None, help='capture the screen to this .mp4')
    ap.add_argument('--tail', type=float, default=2.0, help='seconds to keep recording after the last move')
    a = ap.parse_args()

    pos = board.positions(a.game)
    mv = board.moves(a.game)
    anim = animations(pos)
    end = min(a.to or len(mv), len(mv))
    hwnd = desk.find_window()
    if not hwnd or not desk.focus(hwnd):
        sys.exit('could not bring the game window to the front')
    cur = desk.Cursor()
    cur.zero()
    rec = start_recording(a.record) if a.record else None
    log = []
    t_start = time.perf_counter()
    last_click = 0.0
    stopped = None
    for k in range(a.start, end + 1):
        if desk.user32.GetForegroundWindow() != hwnd:
            stopped = f'the game lost the foreground before move {k}; stopped without clicking'
            break
        before = pos[k - 1]
        target = board.block_px(before[mv[k - 1]])
        boom = anim[k - 1]  # the board animates after this move: wait until it is still
        cur.goto(*target)
        time.sleep(max(0.0, a.gap - (time.perf_counter() - last_click)))
        time.sleep(0.017)  # let the game see the cursor arrive before the press
        last_click = time.perf_counter()
        cur.click()
        hide = hand_box(cur)
        time.sleep(a.settle if boom else 0.03)
        tries, prev, ok = 0, None, False
        deadline = time.perf_counter() + 3.0
        while time.perf_counter() < deadline:
            g = screen_board()
            tries += 1
            stable = prev is not None and np.array_equal(g, prev)
            prev = g
            if boom and not stable:
                continue  # an explosion move: wait until two screenshots agree
            bad = compare(g, pos[k], hide)
            if not bad:
                ok = True
                break
            if stable and time.perf_counter() > deadline - 1.0:
                break
        dt = time.perf_counter() - last_click
        log.append({'move': k, 'explosion': boom, 'secs': round(dt, 3), 'grabs': tries, 'ok': ok})
        if not ok:
            print(f'MISMATCH after move {k} ({tries} screenshots, {dt:.2f}s):')
            for line in bad[:20]:
                print('  ' + line)
            if a.shots:
                os.makedirs(a.shots, exist_ok=True)
                desk.grab().save(os.path.join(a.shots, f'mismatch_{k}.png'))
            break
        if k % 50 == 0:
            el = time.perf_counter() - t_start
            print(f'move {k}: ok, {el:.1f}s elapsed, {el / (k - a.start + 1):.3f}s per move', flush=True)
    el = time.perf_counter() - t_start
    if rec:
        time.sleep(a.tail)
        stop_recording(rec)
    if stopped:
        print('STOPPED: ' + stopped)
    done = log[-1]['move'] if log and log[-1]['ok'] else (log[-1]['move'] - 1 if log else a.start - 1)
    print(f'DONE through move {done} of {end} in {el:.1f}s')
    e = [x['secs'] for x in log if x['explosion'] and x['ok']]
    p = [x['secs'] for x in log if not x['explosion'] and x['ok']]
    if e and p:
        print(f'median click-to-checked: {statistics.median(p):.3f}s for {len(p)} splits, '
              f'{statistics.median(e):.3f}s for {len(e)} moves with an animation')
    if a.log:
        with open(a.log, 'w') as f:
            json.dump(log, f)


if __name__ == '__main__':
    main()
