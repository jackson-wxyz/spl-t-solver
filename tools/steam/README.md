# Playing a solver game on the Steam version, and recording it

Scripts that play a game from `runs/best/` on SPL-T in the Simogo Legacy Collection (Steam,
app 3817040) and record the screen. Tested on 2026-11-15: moves 1 to 226 of the 3898 game
played with every position checked against the engine, the last 76 of them recorded.

| File | What it is |
|---|---|
| `desk.py` | Find the game window, screenshots, mouse and keyboard input (ctypes + Pillow). `Cursor` steers the game's own pointer. |
| `board.py` | Where the board is on screen, and the game's positions from the engine (`frames`). |
| `play.py` | The runner: click each move, check the screen against the engine, optionally record. |
| `video.py` | The 1080p copy of a recording (whole screen, or `--board` for SPL-T's screen only). |

## How it works

- **The game's pointer.** The collection locks the Windows cursor and draws its own hand,
  moved by raw mouse motion. So `SetCursorPos` does nothing; the scripts send relative
  motion with `SendInput`. The hand moves exactly 0.5 screen pixels per unit of motion,
  whatever the step size, and stops at the screen's top-left corner, so pushing it far up
  and left is a known zero. Any target is reached from there exactly.
- **Board geometry** at 3840 x 2160 fullscreen: the board is x 1152 to 2688, y 120 to 2104,
  cells 192 x 124 px. Edges are 1-pixel white lines, point blocks are hatched with a
  number, empty cells are white. A different screen resolution needs these remeasured
  (and probably the pointer gain too).
- **Each move** clicks the centre of the block the engine's game splits, then reads the
  board from a screenshot: each cell free, point or empty, and the edges between free
  blocks. It must match the engine's position after that move before the next click. A
  mismatch is retried for up to 3 seconds and then stops the run with a screenshot.
- **Timing.** Clicks are at least 0.1 s apart. After a move that explodes a cluster (1,139
  in the 3898 game) or drops a block into a hole (7 more), the game blinks the cluster for
  about half a second and lets the blocks fall for about 0.3 s; the runner waits until two
  screenshots agree. Measured: a plain split takes 0.13-0.16 s from click to checked board,
  plus about 0.07 s of pointer travel; an animated move 1.1-1.2 s. **A full 3898 run should
  take about 32 minutes.** The animations alone are about 17 minutes of that.
- **Safety.** Before every move the runner checks that the game is the foreground window,
  and stops without clicking if it is not (otherwise the clicks would land in whatever is in
  front). Nothing else may use the screen or the mouse during a run.
- **Recording.** ffmpeg's Desktop Duplication grabber (`ddagrab`) at 30 fps, encoded on the
  GPU with NVENC at the screen's own 4K (about 4 Mbit/s, so roughly 1 GB for a full run),
  then `video.py` makes the 1080p copy (8-14x real time). Scaling during the capture was
  slightly slower than real time on the CPU, and ffmpeg's D3D11 scaler fails on this driver.
  **No game audio**: this PC has no loopback recording device for ffmpeg. For sound, record
  with the NVIDIA app at the same time (Alt+F9) or enable a "Stereo Mix" device.
- Desktop Duplication fails when started from Git Bash inside the Claude desktop app (its
  sandbox), so run the runner from PowerShell.

## A full run

1. Open the collection, SPL-T, **New Game**, and tap the SPL-T logo: the board is one black
   block, 0 splits. Turn off the NVIDIA performance overlay (Alt+R) if it is showing; it is
   in the recording otherwise (the `--board` copy leaves it out).
2. In PowerShell (one command, no `&&`; Windows PowerShell 5.1 has none):

   ```
   python "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\tools\steam\play.py" --from 1 --record "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam\run_3898.mp4" --log "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam\run_3898.json" --shots "C:\Users\jacks\Claude Code Workspace\Videogames\spl-t-solver\runs\steam"
   ```

   Then leave the mouse alone for about 32 minutes. If it stops, the message says after
   which move; once the game shows that position again it can carry on with
   `--from <next move>` (into a second recording).
3. `python video.py run_3898.mp4 run_3898_1080p.mp4` (add `--board` for SPL-T's screen only,
   810 x 1080).

Recordings go to `runs/steam/`, which git ignores.
