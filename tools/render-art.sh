#!/usr/bin/env bash
# Re-renders Clamber's pre-rendered art from the Blender scripts in art/
# into content/base (or the content folder given). The scripts are the
# source of truth; what they write is checked in, so neither CI nor
# players need Blender.
#
#   tools/render-art.sh [--quick] [content folder]
#
# --quick renders with few samples, for trying out changes. Needs Blender
# 4 (`blender` on the PATH, or $BLENDER).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
blender=${BLENDER:-blender}
quick=()
content="$root/content/base"
for arg in "$@"; do
    case "$arg" in
        --quick) quick=(--quick) ;;
        -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) content=$(realpath "$arg") ;;
    esac
done

for script in "$root"/art/*.py; do
    echo "rendering $(basename "$script")"
    "$blender" -b --factory-startup -P "$script" -- "$content" "${quick[@]}" \
        > "${TMPDIR:-/tmp}/render-art-$(basename "$script" .py).log" 2>&1 \
        || { tail -30 "${TMPDIR:-/tmp}/render-art-$(basename "$script" .py).log"; exit 1; }
    grep '^wrote ' "${TMPDIR:-/tmp}/render-art-$(basename "$script" .py).log"
done
