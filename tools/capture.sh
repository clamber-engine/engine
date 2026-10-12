#!/usr/bin/env bash
# Builds the game and captures the README's gameplay GIF and screenshots
# headlessly into the folder given as the first argument.
#
# Plays the opening of the first level, from the title screen.
set -euo pipefail

out=${1:?usage: tools/capture.sh <output-dir>}
root=$(cd "$(dirname "$0")/.." && pwd)

cargo build --release --locked -p clamber-game --manifest-path "$root/Cargo.toml"
"$root/target/release/clamber-game" \
    --capture "$root/content/base/levels/first.level" \
    --inputs "$root/tools/capture/first-tour.scn" \
    --out "$out"
