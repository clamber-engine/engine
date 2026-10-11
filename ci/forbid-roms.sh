#!/usr/bin/env bash
# Fail if the tree contains ROM images or other ROM-derived dumps, by
# extension or by a size typical of a cartridge dump (with or without a
# 512-byte copier header).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0
while IFS= read -r -d '' f; do
  case "${f,,}" in
    *.sfc|*.smc|*.swc|*.fig|*.spc)
      echo "ROM-like file extension: $f"; fail=1; continue ;;
  esac
  size=$(stat -c %s "$f")
  for mib in 1 2 3 4 6 8; do
    rom=$((mib * 1048576))
    if [ "$size" -eq "$rom" ] || [ "$size" -eq $((rom + 512)) ]; then
      echo "File size matches a cartridge dump (${mib} MiB): $f"; fail=1
    fi
  done
done < <(git ls-files -z)

exit $fail
