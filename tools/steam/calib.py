"""Measure where SPL-T's board is and how the game's pointer moves, for the current window.

usage (PowerShell, game in front showing SPL-T's board with 0 splits): python calib.py

Writes calib.json next to this file; play.py reads it. The board is found from the score
bar's dotted line (board.find_geom), the pointer by moving the hand to two places on the
empty black board and finding it there (desk.find_hand). Nothing is clicked.
"""
import sys
import time

import board
import desk


def calibrate_cursor(rect, client, scale=1.0, verbose=True):
    """Fit the pointer's gain and zero point by moving the hand to two places inside
    `rect` (screen pixels) and finding it there. `rect` must be dark or plain blue, with no
    white patches: the empty SPL-T board, or the collection's background beside the tablet.
    `client`: the game window's client rectangle. `scale`: the window's size relative to
    3840 x 2160, for the first guesses of the gain."""
    cur = desk.Cursor()
    l, t, r, b = rect
    w, h = r - l, b - t
    pts = [(l + 0.3 * w, t + 0.3 * h), (l + 0.7 * w, t + 0.7 * h)]
    found = None
    for g in (0.5, 0.5 * scale, 0.5 / scale, 0.25, 1.0):
        # Guess: the hand's zero is the client area's top-left corner.
        cur.gain, cur.ox, cur.oy = g, float(client[0]), float(client[1])
        cur.zero()
        meas = []
        for (x, y) in pts:
            cur.goto(x, y)
            time.sleep(0.25)
            meas.append((cur.ux, cur.uy, desk.find_hand(desk.grab(), rect)))
        if all(m[2] is not None for m in meas) and meas[1][2][0] - meas[0][2][0] > 0.2 * w:
            found = meas
            break
        if verbose:
            print(f'gain guess {g:.3f}: hand not found at both points')
    if found is None:
        sys.exit('could not find the pointer')
    (u1, v1, a1), (u2, v2, a2) = found
    gx = (a2[0] - a1[0]) / (u2 - u1)
    gy = (a2[1] - a1[1]) / (v2 - v1)
    gain = (gx + gy) / 2
    cur.gain, cur.ox, cur.oy = gain, a1[0] - gain * u1, a1[1] - gain * v1
    # Check: send the hand to a third point and see where it lands.
    tx, ty = l + 0.5 * w, t + 0.5 * h
    cur.goto(tx, ty)
    time.sleep(0.25)
    a3 = desk.find_hand(desk.grab(), rect)
    err = None if a3 is None else (a3[0] - tx, a3[1] - ty)
    if verbose:
        print(f'pointer: gain {gx:.4f} / {gy:.4f} px per unit, zero at ({cur.ox:.1f}, {cur.oy:.1f}); check error {err}')
    if err is None or abs(err[0]) > 6 or abs(err[1]) > 6:
        sys.exit('pointer check failed')
    return cur


def board_rect(geom):
    l, t, r, b = geom.region
    return (l + 10, t + 10, r - 10, b - 10)


def main():
    hwnd = desk.find_window()
    if not hwnd or not desk.focus(hwnd):
        sys.exit('could not bring the game window to the front')
    client = desk.client_rect(hwnd)
    # Move the hand off the board first, so it does not hide the score bar's line.
    c0 = desk.Cursor()
    c0.zero()
    time.sleep(0.2)
    geom = board.find_geom(desk.grab(), client)
    if geom is None:
        sys.exit("could not find SPL-T's board (is a game open, at 0 splits?)")
    print(f'client {client}; board at ({geom.x0:.1f}, {geom.y0:.1f}), cells {geom.cw:.2f} x {geom.ch:.2f}, '
          f'region {geom.region}, screen {geom.screen}')
    cur = calibrate_cursor(board_rect(geom), client, geom.scale)
    board.save_calib(geom, cur.to_dict())
    cur.goto(geom.region[2] + 40, geom.region[3] - 40)  # park the hand
    print('wrote', board.CALIB)


if __name__ == '__main__':
    main()
