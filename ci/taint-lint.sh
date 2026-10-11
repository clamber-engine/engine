#!/usr/bin/env bash
# Flag code-shaped material that suggests knowledge of the original
# program's internals: console memory addresses and 65816 assembly.
# Findings need human review; a match is not proof of contamination.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Long-addressed bank:offset ($80:8000), WRAM addresses ($7E1234 / 0x7E1234),
# and lines that read as 65816 assembly.
patterns=(
  '\$[0-9A-Fa-f]{2}:[0-9A-Fa-f]{4}'
  '(\$|0x)7[EeFf][0-9A-Fa-f]{4}\b'
  '^\s*(LDA|STA|LDX|STX|LDY|STY|JSL|JSR|RTL|RTS|REP|SEP|PHB|PLB|XBA|TCD|MVN|MVP)(\.[bwl])?\s'
)

fail=0
for p in "${patterns[@]}"; do
  if git grep -nIE "$p" -- . ':!ci/' ':!.claude/hooks/'; then
    fail=1
  fi
done

[ $fail -eq 0 ] || echo "taint-lint: the lines above need clean-room review." >&2
exit $fail
