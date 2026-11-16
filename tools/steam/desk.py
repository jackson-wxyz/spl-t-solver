"""Desktop helpers for driving the Steam version of SPL-T (Simogo Legacy Collection):
find its window, take screenshots, move the mouse and click. Windows only; ctypes and
Pillow, nothing else.

All coordinates are physical screen pixels (the process declares itself DPI aware, so a
4K screen at 200% scaling is 3840 x 2160 here).
"""
import ctypes
import ctypes.wintypes as wt
import time

from PIL import ImageGrab

user32 = ctypes.windll.user32
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # per-monitor aware
except OSError:
    user32.SetProcessDPIAware()

GAME_TITLE = 'Simogo Legacy Collection'


def find_window(title=GAME_TITLE):
    """Handle of the first visible top-level window whose title contains `title`."""
    found = []

    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            n = user32.GetWindowTextLengthW(hwnd)
            buf = ctypes.create_unicode_buffer(n + 1)
            user32.GetWindowTextW(hwnd, buf, n + 1)
            if title.lower() in buf.value.lower():
                found.append((hwnd, buf.value))
        return True

    user32.EnumWindows(cb, 0)
    return found[0][0] if found else None


def client_rect(hwnd):
    """(left, top, right, bottom) of the window's client area, in screen pixels."""
    r = wt.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    p = wt.POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(p))
    return (p.x, p.y, p.x + r.right, p.y + r.bottom)


def focus(hwnd):
    """Bring the window to the front. A synthetic Alt press first lifts Windows' lock on
    background processes changing the foreground window."""
    VK_MENU, KEYUP = 0x12, 0x0002
    user32.keybd_event(VK_MENU, 0, 0, 0)
    user32.keybd_event(VK_MENU, 0, KEYUP, 0)
    user32.ShowWindow(hwnd, 9)  # SW_RESTORE
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.15)
    return user32.GetForegroundWindow() == hwnd


def grab(rect=None):
    """Screenshot of `rect` (screen pixels), or of the whole screen."""
    return ImageGrab.grab(bbox=rect, all_screens=True)


# ---- mouse and keyboard, via SendInput so the game sees real input ----

class _MI(ctypes.Structure):
    _fields_ = [('dx', wt.LONG), ('dy', wt.LONG), ('mouseData', wt.DWORD), ('dwFlags', wt.DWORD),
                ('time', wt.DWORD), ('dwExtraInfo', ctypes.POINTER(ctypes.c_ulong))]


class _KI(ctypes.Structure):
    _fields_ = [('wVk', wt.WORD), ('wScan', wt.WORD), ('dwFlags', wt.DWORD), ('time', wt.DWORD),
                ('dwExtraInfo', ctypes.POINTER(ctypes.c_ulong))]


class _U(ctypes.Union):
    _fields_ = [('mi', _MI), ('ki', _KI)]


class _INPUT(ctypes.Structure):
    _fields_ = [('type', wt.DWORD), ('u', _U)]


def _send(*inputs):
    arr = (_INPUT * len(inputs))(*inputs)
    user32.SendInput(len(inputs), arr, ctypes.sizeof(_INPUT))


def _mouse(flags):
    i = _INPUT()
    i.type = 0
    i.u.mi = _MI(0, 0, 0, flags, 0, None)
    return i


def move(x, y):
    user32.SetCursorPos(int(round(x)), int(round(y)))


def click(x, y, hold=0.03):
    """Move to (x, y) and click the left button."""
    move(x, y)
    time.sleep(0.01)
    _send(_mouse(0x0002))  # LEFTDOWN
    time.sleep(hold)
    _send(_mouse(0x0004))  # LEFTUP


def key(vk, hold=0.03):
    down = _INPUT()
    down.type = 1
    down.u.ki = _KI(vk, 0, 0, 0, None)
    up = _INPUT()
    up.type = 1
    up.u.ki = _KI(vk, 0, 0x0002, 0, None)
    _send(down)
    time.sleep(hold)
    _send(up)


def rel(dx, dy):
    """Relative mouse motion, as a real mouse would send it."""
    i = _INPUT()
    i.type = 0
    i.u.mi = _MI(int(dx), int(dy), 0, 0x0001, 0, None)  # MOVE (relative)
    _send(i)


class Cursor:
    """The Legacy Collection's own pointer (the "right cursor").

    The game locks the Windows cursor and moves its own hand sprite by the raw motion of
    the mouse. Measured on 2026-11-15 at 3840 x 2160 fullscreen: the hand moves exactly
    0.5 screen pixels per unit of motion, whatever the step size, and stops at the screen's
    top-left corner. So pushing it far up and left is a known zero, and from there a
    target's position is reached exactly. `gain` (pixels per unit) and the hotspot's
    position at the zero (ox, oy) depend on the window; `calib.calibrate_cursor` measures
    them. Moving the real mouse while this runs breaks the tracking (call `zero` again)."""

    STEP = 60  # units per input event; the game does not care, but keep events small

    def __init__(self, gain=0.5, ox=0.0, oy=0.0):
        self.gain, self.ox, self.oy = gain, ox, oy
        self.ux = self.uy = None  # position in motion units since the last zero

    def _go(self, dx, dy):
        n = max(1, -(-max(abs(dx), abs(dy)) // self.STEP))
        sx = sy = 0
        for k in range(1, n + 1):
            tx, ty = round(dx * k / n), round(dy * k / n)
            rel(tx - sx, ty - sy)
            sx, sy = tx, ty
            time.sleep(0.002)

    def zero(self):
        self._go(-14000, -14000)
        self.ux = self.uy = 0
        time.sleep(0.05)

    def units_for(self, x, y):
        return round((x - self.ox) / self.gain), round((y - self.oy) / self.gain)

    def goto_units(self, tx, ty):
        if self.ux is None:
            self.zero()
        self._go(tx - self.ux, ty - self.uy)
        self.ux, self.uy = tx, ty

    def goto(self, x, y):
        """Move the hand's hotspot to screen pixel (x, y)."""
        self.goto_units(*self.units_for(x, y))

    def pos(self):
        """Where the hotspot is now, in screen pixels."""
        return self.ox + self.gain * self.ux, self.oy + self.gain * self.uy

    def click(self, x=None, y=None, hold=0.03):
        if x is not None:
            self.goto(x, y)
        _send(_mouse(0x0002))  # LEFTDOWN
        time.sleep(hold)
        _send(_mouse(0x0004))  # LEFTUP

    def to_dict(self):
        return {'gain': self.gain, 'ox': self.ox, 'oy': self.oy}


def find_hand(img, rect):
    """The hand sprite's fingertip (top centre of its white glove) inside `rect` (screen
    pixels) of a screenshot `img`, or None. Looks for a dense patch of near-white pixels,
    which nothing else on a dark SPL-T board has (lines are one pixel, empty cells are
    white rectangles, so call this only where none are)."""
    import numpy as np
    l, t, r, b = rect
    a = np.asarray(img.crop(rect).convert('RGB'), dtype=np.int16)
    white = a.min(axis=2) > 225
    T = 8
    hh, ww = white.shape[0] // T * T, white.shape[1] // T * T
    tiles = white[:hh, :ww].reshape(hh // T, T, ww // T, T).mean(axis=(1, 3))
    ys, xs = np.nonzero(tiles > 0.6)
    if len(xs) < 3:
        return None
    y0, y1 = max(0, (ys.min() - 2) * T), min(white.shape[0], (ys.max() + 3) * T)
    x0, x1 = max(0, (xs.min() - 2) * T), min(white.shape[1], (xs.max() + 3) * T)
    box = white[y0:y1, x0:x1]
    wy, wx = np.nonzero(box)
    top = wy.min()
    tip = wx[wy <= top + 3].mean()
    return (l + x0 + tip, t + y0 + top)


def shot(path, rect=None, scale=0.5):
    """Save a (downscaled) screenshot for a person or a model to look at."""
    im = grab(rect)
    if scale != 1:
        im = im.resize((max(1, int(im.width * scale)), max(1, int(im.height * scale))))
    im.save(path)
    return im.size


if __name__ == '__main__':
    import sys
    h = find_window()
    print('window', h, client_rect(h) if h else None)
    if len(sys.argv) > 1:
        print('saved', shot(sys.argv[1], client_rect(h) if h else None, float(sys.argv[2]) if len(sys.argv) > 2 else 0.5))
