# Playing a solver game on the Steam version, and recording it

Scripts that play a game from `runs/best/` on SPL-T in the Simogo Legacy Collection (Steam,
app 3817040), check every position on screen against the engine, and record SPL-T's screen
for a video.

| File | What it is |
|---|---|
| `desk.py` | Find the game window, screenshots, mouse and keyboard input (ctypes + Pillow). `Cursor` steers the game's own pointer; `find_hand` finds it on screen. |
| `board.py` | Where the board is on screen (`find_geom`), and the game's positions from the engine (`frames`). |
| `calib.py` | Measures the board and the pointer for the current window and writes `calib.json`. |
| `play.py` | The runner: click each move, check the screen against the engine, optionally record. |
| `video.py` | Finishing a recording: enlarge by pixel repetition, crop, pad to 9:16. |

## How it works

- **The game's pointer.** The collection locks the Windows cursor and draws its own hand,
  moved by raw mouse motion, so `SetCursorPos` does nothing; the scripts send relative
  motion with `SendInput`. The hand moves exactly 0.5 screen pixels per unit of motion
  (fullscreen 4K and a 1920 x 1080 window alike) and stops at the window's top-left corner,
  which gives a known zero. `calib.calibrate_cursor` measures both by moving the hand to
  two points on the empty board and finding the white glove there.
- **Board geometry.** SPL-T's screen keeps the iPad's proportions: board 8 cells wide and
  31/24 of its width tall, with the score bar (1/24 of the width) above it. The score bar
  ends in a dashed one-pixel line (two pixels on, two off, at any window size) whose ends are
  the board's edges, so `board.find_geom` finds everything from that line, even at 0
  splits. Fullscreen 4K: board at (1152, 120), cells 192 x 124. A 1920 x 1080 window on the
  4K desktop: SPL-T's screen is 768 x 1024, cells exactly 96 x 62.
- **The game draws real one-pixel lines at any size.** Block edges, digits and hatching are
  one screen pixel wide whether SPL-T's screen is 1536 or 768 pixels wide. Scaling a
  recording down smears them to grey, so for video it is better to make the game render
  small and enlarge the recording by pixel repetition: 768 x 1024 doubled gives 1536 x 2048
  with sharp two-pixel lines, which survive a phone's or YouTube's scaling (and are about as
  thick, relative to the board, as on the iPhone).
- **Each move** clicks the centre of the block the engine's game splits, then reads the
  board from a screenshot: each cell free, point or empty, and the edges between free
  blocks. It must match the engine's position after that move before the next click. A
  mismatch is retried for up to 3 seconds and then stops the run with a screenshot.
- **Timing.** Clicks are at least 0.1 s apart. After a move that explodes a cluster (1,139
  in the 3898 game) or drops a block into a hole (7 more) the game blinks the cluster for
  about half a second and lets the blocks fall for about 0.3 s; the runner waits until two
  screenshots agree. In the window: a plain split takes 0.12 s from click to checked board,
  plus about 0.07 s of pointer travel; an animated move about 1.05 s. A full 3898 run takes
  about 30 minutes; the animations alone are about 17 of them.
- **Safety.** Before every move the runner checks that the game is the foreground window
  and stops without clicking if it is not (otherwise the clicks would land in whatever is in
  front). Nothing else may use the screen or the mouse during a run, including other Claude
  sessions that launch games.
- **Recording.** ffmpeg's Desktop Duplication grabber (`ddagrab`), cropped to SPL-T's
  screen, 30 fps, encoded on the GPU with NVENC (about 2 Mbit/s at 768 x 1024). ffmpeg's
  D3D11 scaler fails on this driver, so any scaling happens afterwards in `video.py`.
  **No game audio**: this PC has no loopback recording device for ffmpeg. For sound, record
  with the NVIDIA app at the same time (Alt+F9) or enable a "Stereo Mix" device.
- Desktop Duplication fails when started from Git Bash inside the Claude desktop app (its
  sandbox), so start anything that records from PowerShell.

## A full run

1. **Window.** Unity keeps the last window settings in the registry
   (`HKCU\Software\Simogo\Simogo Legacy Collection`, `Screenmanager ...`). To get a
   1920 x 1080 window, start the game once with Unity's options (Steam asks to confirm
   them):

   ```
   & "C:\Program Files (x86)\Steam\steamapps\common\Simogo Legacy Collection\Simogo Legacy Collection.exe" -screen-fullscreen 0 -screen-width 1920 -screen-height 1080
   ```

   Back to fullscreen 4K afterwards: the same with `-screen-fullscreen 1 -screen-width 3840 -screen-height 2160`.
2. **Game.** In the collection, open SPL-T, **New Game** (OK to the reset question), and tap
   the SPL-T logo: the board is one black block, 0 splits. (From a game in progress: Esc,
   OK, which goes back to the collection's home screen, then SPL-T again.)
3. **Run**, in PowerShell (one command, no `&&`; Windows PowerShell 5.1 has none):

   ```
   python "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\tools\steam\play.py" --from 1 --calibrate --record "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam\run_3898.mp4" --log "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam\run_3898.json" --shots "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam"
   ```

   Then leave the mouse alone for about 30 minutes. If it stops, the message says after
   which move; with the game still showing that position it can carry on with
   `--from <next move>` (into a second recording; `calib.json` is reused).
4. **Finish**: `python video.py run_3898.mp4 run_3898_1536x2048.mp4 --upscale 2` (add
   `--pad916` for a 9:16 frame).

Recordings go to `runs/steam/`, which git ignores.
