#!/usr/bin/env bash
# Builds the game and captures the README's gameplay GIF and screenshots
# headlessly into the folder given as the first argument.
#
# Plays the sandbox level, with the collision view on since it has no
# terrain art, until the first real level (#21) has its own playthrough
# script.
set -euo pipefail

out=${1:?usage: tools/capture.sh <output-dir>}
root=$(cd "$(dirname "$0")/.." && pwd)

cargo build --release --locked -p clamber-game --manifest-path "$root/Cargo.toml"
"$root/target/release/clamber-game" \
    --capture "$root/content/levels/sandbox.level" \
    --inputs "$root/tools/capture/sandbox-tour.scn" \
    --out "$out" \
    --debug
