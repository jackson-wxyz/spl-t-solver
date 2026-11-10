#!/usr/bin/env bash
# Promote the longest game found so far: verify it, copy it to runs/best/, export it for the
# hand-replay page, and rebuild the visualization data. Safe to run while workers are busy.
#
# usage: tools/queue/finalize.sh
set -eu
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --bin replay --bin stats --bin export_game >/dev/null 2>&1 || cargo build --release --bin replay --bin stats --bin export_game
best=$(python -I tools/queue/best_of.py)
line=$(./target/release/replay "$best" | tail -1)
echo "$line"
case "$line" in *"game over - matches"*) ;; *) echo "verification failed for $best"; exit 1 ;; esac
n=$(echo "$line" | sed -E 's/.*: ([0-9]+) splits replayed.*/\1/')
# Second opinion from the original Python emulator, when a checkout of it is at hand.
for ref in "josh holder SPLT reference/SPLT" reference/josh-holder-SPLT; do
  if [ -f "$ref/core_update.py" ]; then python tools/analysis/pyreplay.py "$ref" "$best" | tail -1; break; fi
done
dest="runs/best/game_${n}.txt"
cp "$best" "$dest"
./target/release/export_game "$dest" "runs/best/game_${n}.json"
python -I tools/analysis/build_data.py --game "$dest"
echo "best game: $dest ($n splits); replay JSON: runs/best/game_${n}.json; page data rebuilt in tools/viz/data.js"
