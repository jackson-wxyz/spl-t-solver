# Animated boards for the blog post

The post "A Clockwork of Halvings" (jwxyz, `content/Posts/spl-t-clockwork.md`) shows the
record game as animations. They are not videos: the page replays the game's move list with
a JavaScript copy of the rules and draws each position on a canvas.

| File | What it is |
|---|---|
| `splt-engine.js` | The rules in JavaScript, a port of the reference path of `src/engine.rs`. |
| `splt-player.js` | Draws boards and runs the animations (`figure.splt` elements); also renders a board as SVG. |
| `build.mjs` | Writes the blog's `static/splt/splt.js` (move lists + engine + player) and one still SVG per animation, shown where JavaScript does not run. |
| `verify.mjs` | Checks the JavaScript engine against the Rust engine, position by position. |
| `preview.mjs` | Rough local preview of the post for a machine without Hugo. |

## Commands

Run from the solver repo's root; `BLOG` is the blog's folder.

```
node tools/blog/build.mjs "BLOG"
node tools/blog/preview.mjs "BLOG" OUT_DIR
python -m http.server 8772 --directory OUT_DIR
```

To check the JavaScript engine after changing it, dump the Rust engine's positions with
`frames` and compare:

```
cargo run --release --bin frames -- runs/best/game_3898.txt > frames_3898.txt
node tools/blog/verify.mjs runs/best/game_3898.txt frames_3898.txt
```

As of 2026-11-10 it matches on all 12,943 positions of five games (the 3898, 3103, 2670 and
2064 games and Josh Holder's 1208 game).

## In the post

The blog's `layouts/shortcodes/splt.html` writes the markup. Parameters: `name` (the still
image's file name), `game` (ids from `GAMES` in `build.mjs`, comma separated for boards side
by side), `from`, `to`, `at` (the move the still shows), `speed`, `mode`, `cols`, `labels`,
`legend`, `titles`, `alt`, `caption`.

Modes: `lapse` plays `speed` moves a second with no in-between frames, and exploding cells
leave a short glow. `tween` plays one move at a time and slides blocks when they fall.
`events` jumps from explosion to explosion and leaves out the cuts in between.

To feature another game, add it to `GAMES` in `build.mjs`, run the build, and change the
move ranges in the post's shortcodes (they are specific to the 3898 game).
