"""Play a solver game on the Steam version of SPL-T (Simogo Legacy Collection), checking
the screen against the engine after every move.

usage: python play.py --from K [--to N] [--calibrate] [--record FILE.mp4 [--full-screen]]
                      [--game runs/best/game_3898.txt] [--gap 0.10] [--log FILE] [--shots DIR]

Run it from PowerShell, not from Git Bash inside the Claude desktop app: screen capture
(Desktop Duplication, used by --record) fails from that sandbox.

The game must be open at the position after K - 1 moves (for a new game: K = 1 and the
board one black block). Where the board is and how the game's pointer moves come from
calib.json (see calib.py); --calibrate measures them first, which needs the board at 0
splits. Without either, the 3840 x 2160 fullscreen layout is assumed.

Move k clicks the centre of the block the game splits at move k, waits, takes a screenshot
and compares it with the engine's position after move k: which cells are free, point or
empty, and where the edges between free blocks are. After a move that explodes a cluster
or drops a block into a hole, it first waits for the board to stop changing (about 0.9 s).
On a mismatch it retries for 3 s (an animation may still be running), then stops and saves
the screenshot.

--record: capture SPL-T's screen (score bar and board, 3:4, at the screen's own pixels)
with ffmpeg (Desktop Duplication, NVIDIA encoder, 30 fps) from just before the first move
to --tail seconds after the last. --full-screen records the whole screen instead.

The run stops if the game is no longer the foreground window (the clicks would otherwise
land in whatever window is in front). Do not touch the mouse while it runs: the game's
pointer is steered by relative mouse motion, and a nudge from a real mouse puts every later
click in the wrong place.
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

W, H = board.W, board.H
FREE, POINT, VOID = 0, 1, 2
G = board.GEOM_4K  # set in main()


def screen_board():
    """The board region as a grayscale array (origin at the board's corner)."""
    return np.asarray(desk.grab(G.region).convert('L'), dtype=np.uint8)


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
    cw, ch = G.cw, G.ch
    x0, x1 = int(cx * cw + cw * 0.15), int(cx * cw + cw * 0.85)
    y0, y1 = int(cy * ch + ch * 0.2), int(cy * ch + ch * 0.8)
    f = (g[y0:y1, x0:x1] > 100).mean()
    return (FREE if f < 0.01 else VOID if f > 0.9 else POINT), f


def vline(g, cx, cy):
    """Is there an edge line between cell (cx, cy) and (cx + 1, cy)?"""
    x = int(round((cx + 1) * G.cw))
    y0, y1 = int(cy * G.ch + G.ch * 0.25), int(cy * G.ch + G.ch * 0.75)
    strip = g[y0:y1, max(0, x - 2):x + 3].max(axis=1)
    return (strip > 100).mean() > 0.8


def hline(g, cx, cy):
    """Is there an edge line between cell (cx, cy) and (cx, cy + 1)?"""
    y = int(round((cy + 1) * G.ch))
    x0, x1 = int(cx * G.cw + G.cw * 0.25), int(cx * G.cw + G.cw * 0.75)
    strip = g[max(0, y - 2):y + 3, x0:x1].max(axis=0)
    return (strip > 100).mean() > 0.8


def compare(g, blocks, hide=None):
    """Differences between the screen and an engine position, as a list of strings.
    `hide`: a rectangle in board pixels (the hand) where nothing is checked."""
    typ, grid = expected(blocks)
    cw, ch = G.cw, G.ch

    def hidden(x0, y0, x1, y1):
        return hide is not None and x0 < hide[2] and x1 > hide[0] and y0 < hide[3] and y1 > hide[1]

    bad = []
    for cy in range(H):
        for cx in range(W):
            if hidden(cx * cw, cy * ch, (cx + 1) * cw, (cy + 1) * ch):
                continue
            t, f = cell_type(g, cx, cy)
            if t != typ[cy, cx]:
                bad.append(f'cell ({cx},{cy}) looks {"FPV"[t]} ({f:.2f}), expected {"FPV"[typ[cy, cx]]}')
    for cy in range(H):
        for cx in range(W - 1):
            if typ[cy, cx] == FREE and typ[cy, cx + 1] == FREE and not hidden((cx + 1) * cw - 4, cy * ch, (cx + 1) * cw + 4, (cy + 1) * ch):
                want = grid[cy, cx] != grid[cy, cx + 1]
                if vline(g, cx, cy) != want:
                    bad.append(f'edge right of ({cx},{cy}): {"missing" if want else "unexpected"}')
    for cy in range(H - 1):
        for cx in range(W):
            if typ[cy, cx] == FREE and typ[cy + 1, cx] == FREE and not hidden(cx * cw, (cy + 1) * ch - 4, (cx + 1) * cw, (cy + 1) * ch + 4):
                want = grid[cy, cx] != grid[cy + 1, cx]
                if hline(g, cx, cy) != want:
                    bad.append(f'edge below ({cx},{cy}): {"missing" if want else "unexpected"}')
    return bad


def animations(pos):
    """For every move, whether the board animates after it: a cluster explodes (some point
    block is worth 1 before the move), or blocks fall without an explosion (a vertical split
    that leaves half a block over a hole; 7 moves in the 3898 game)."""
    out = []
    for k in range(1, len(pos)):
        before, after = pos[k - 1], pos[k]
        boom = any(b[4] == 1 for b in before)
        if not boom:
            # A fall moves a block: more than the split's two halves are new geometry.
            prev = {(x, y, w, h) for (x, y, w, h, p) in before}
            boom = len([b for b in after if (b[0], b[1], b[2], b[3]) not in prev]) > 2
        out.append(boom)
    return out


def hand_box(cur):
    """The hand sprite's rectangle in board pixels (generous)."""
    hx, hy = cur.pos()
    s = G.scale
    l, t = G.region[:2]
    return (hx - l - 30 * s, hy - t - 20 * s, hx - l + 100 * s, hy - t + 125 * s)


def ffmpeg():
    """Path of ffmpeg (on PATH, or where winget puts it)."""
    return shutil.which('ffmpeg') or os.path.join(os.environ.get('LOCALAPPDATA', ''), 'Microsoft', 'WinGet', 'Links', 'ffmpeg.exe')


def start_recording(path, crop=None):
    """ffmpeg capturing the screen (Desktop Duplication) to an H.264 file, encoded on the
    GPU. `crop`: (left, top, width, height) in screen pixels, or None for the whole screen.
    Returns the process; `stop_recording` ends it."""
    src = 'ddagrab=output_idx=0:framerate=30:draw_mouse=0'
    if crop:
        x, y, w, h = crop
        src += f':video_size={w}x{h}:offset_x={x}:offset_y={y}'
    cmd = [ffmpeg(), '-y', '-hide_banner', '-loglevel', 'warning', '-f', 'lavfi', '-i', src,
           '-c:v', 'h264_nvenc', '-preset', 'p6', '-tune', 'hq', '-rc', 'vbr', '-cq', '16', '-b:v', '0',
           '-g', '60', '-profile:v', 'high', path]
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
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        p.kill()


def main():
    global G
    ap = argparse.ArgumentParser()
    ap.add_argument('--game', default='runs/best/game_3898.txt')
    ap.add_argument('--from', dest='start', type=int, required=True)
    ap.add_argument('--to', type=int, default=None)
    ap.add_argument('--gap', type=float, default=0.10, help='least time between two clicks (s)')
    ap.add_argument('--settle', type=float, default=0.45, help='wait before checking an animated move (s)')
    ap.add_argument('--calibrate', action='store_true', help='measure the board and pointer first (0 splits)')
    ap.add_argument('--log', default=None)
    ap.add_argument('--shots', default=None, help='folder for a screenshot on failure')
    ap.add_argument('--record', default=None, help="capture SPL-T's screen to this .mp4")
    ap.add_argument('--full-screen', action='store_true', help='record the whole screen, not just the game')
    ap.add_argument('--tail', type=float, default=8.0, help='seconds to keep recording after the last move')
    a = ap.parse_args()

    pos = board.positions(a.game)
    mv = board.moves(a.game)
    anim = animations(pos)
    end = min(a.to or len(mv), len(mv))
    hwnd = desk.find_window()
    if not hwnd or not desk.focus(hwnd):
        sys.exit('could not bring the game window to the front')
    if a.calibrate:
        import calib
        if a.start != 1:
            sys.exit('--calibrate needs the board at 0 splits (--from 1)')
        client = desk.client_rect(hwnd)
        desk.Cursor().zero()
        time.sleep(0.2)
        G = board.find_geom(desk.grab(), client)
        if G is None:
            sys.exit("could not find SPL-T's board")
        cur = calib.calibrate_cursor(calib.board_rect(G), client, G.scale)
        board.save_calib(G, cur.to_dict())
    else:
        c = board.load_calib()
        if c:
            G = board.Geom(**c['board'])
            cur = desk.Cursor(**c['cursor'])
        else:
            cur = desk.Cursor()
        cur.zero()
    print(f'board at ({G.x0:.1f}, {G.y0:.1f}), cells {G.cw:.2f} x {G.ch:.2f}; pointer gain {cur.gain:.4f}', flush=True)
    rec = start_recording(a.record, None if a.full_screen else G.screen) if a.record else None
    log = []
    t_start = time.perf_counter()
    last_click = 0.0
    stopped = None
    bad = []
    for k in range(a.start, end + 1):
        if desk.user32.GetForegroundWindow() != hwnd:
            stopped = f'the game lost the foreground before move {k}; stopped without clicking'
            break
        before = pos[k - 1]
        boom = anim[k - 1]  # the board animates after this move: wait until it is still
        cur.goto(*G.block_px(before[mv[k - 1]]))
        time.sleep(max(0.0, a.gap - (time.perf_counter() - last_click)))
        time.sleep(0.017)  # let the game see the pointer arrive before the press
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
                continue  # wait until two screenshots agree
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
        if k % 100 == 0:
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
