#!/usr/bin/env bash
# Clean-room taint guard for Claude Code (PreToolUse).
#
# Blocks the clean side's agents from *acquiring* tainted information:
#   - WebSearch entirely (public reverse-engineering material is one query away)
#   - WebFetch outside an allowlist of general Rust/graphics documentation
#   - Bash/Read/Grep/Glob calls that touch the research org, ROM images,
#     or disassembly/decompilation material
#   - Bash commands that reach the network anywhere other than documentation,
#     crate registries, and this org's GitHub repositories. This holds even
#     for commands that run outside the sandbox.
#
# It deliberately does not scan Write/Edit content: what gets *written* is
# checked by ci/taint-lint.sh, and policy docs need to name these things.
#
# This is a tripwire, not the wall. The wall is environment and credential
# separation (see CONTRIBUTING.md). Exit code 2 blocks the call and the
# message on stderr is shown to the agent.
#
# No dependencies beyond bash, grep and sed.

set -u

RESEARCH_ORG='clamber-research'

# Hosts WebFetch may reach. Keep this to general documentation; do not add
# github.com or wikis, which host game reverse-engineering material.
WEBFETCH_ALLOW=(
  docs.rs
  doc.rust-lang.org
  rust-lang.github.io
  crates.io
  lib.rs
  wgpu.rs
  sotrh.github.io
  www.khronos.org
  registry.khronos.org
)

# Hosts shell commands may reach in addition to WEBFETCH_ALLOW.
BASH_HOST_ALLOW=(
  index.crates.io
  static.crates.io
  static.rust-lang.org
  sh.rustup.rs
)

# GitHub is allowed only for this org's repositories.
CLEAN_ORG='clamber-engine'
GITHUB_HOSTS='^(github\.com|api\.github\.com|raw\.githubusercontent\.com|codeload\.github\.com)$'

TAINT_PATTERN="${RESEARCH_ORG}|\\.(sfc|smc|swc|fig|spc)([^A-Za-z0-9]|\$)|disassembl|decompil"

LOG="${CLAUDE_PROJECT_DIR:-.}/.claude/taint-guard.log"

input="$(cat)"

tool="$(printf '%s' "$input" | sed -n 's/.*"tool_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"

block() {
  printf '%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$tool" "$1" >> "$LOG" 2>/dev/null
  printf 'Blocked by clean-room taint guard: %s\nSee CONTRIBUTING.md. If this is a false positive, ask a human to review it.\n' "$1" >&2
  exit 2
}

host_allowed() {
  local h
  for h in "${WEBFETCH_ALLOW[@]}" "${BASH_HOST_ALLOW[@]}"; do
    [ "$1" = "$h" ] && return 0
  done
  return 1
}

# Checks every URL and every gh invocation in a shell command.
check_bash_network() {
  local cmd="$1" url host path owner

  while IFS= read -r url; do
    [ -n "$url" ] || continue
    host="$(printf '%s' "$url" | sed -n 's#^[A-Za-z]*://\([^/:?#]*\).*#\1#p' | tr 'A-Z' 'a-z')"
    path="$(printf '%s' "$url" | sed -n 's#^[A-Za-z]*://[^/]*\(/[^?#]*\)\{0,1\}.*#\1#p')"
    if printf '%s' "$host" | grep -Eq "$GITHUB_HOSTS"; then
      printf '%s' "$path" | grep -Eq "^/(repos/)?${CLEAN_ORG}(/|\.git|$)" && continue
      block "GitHub access outside ${CLEAN_ORG} ($host$path)"
    fi
    host_allowed "$host" || block "network access to '$host' is not on the allowlist"
  done < <(printf '%s' "$cmd" | grep -Eo '[A-Za-z]+://[^][[:space:]"'"'"'\\<>(){};|&]+')

  # gh: only this org, no search, no gists
  if printf '%s' "$cmd" | grep -Eq '(^|[^A-Za-z0-9_-])gh[[:space:]]'; then
    printf '%s' "$cmd" | grep -Eq '(^|[^A-Za-z0-9_-])gh[[:space:]]+(search|gist|browse)\b' \
      && block "gh search, gist and browse are disabled on the clean side"
    printf '%s' "$cmd" | grep -Eq 'search/(code|repositories|issues|commits|users)' \
      && block "GitHub search is disabled on the clean side"
    while IFS= read -r owner; do
      [ -n "$owner" ] && [ "$owner" != "$CLEAN_ORG" ] && block "gh access outside ${CLEAN_ORG} ('$owner')"
    done < <(printf '%s' "$cmd" | grep -Eo '(-R|--repo)[[:space:]=]+[A-Za-z0-9_.-]+/' | sed -E 's#.*[[:space:]=]##; s#/$##'
             printf '%s' "$cmd" | grep -Eo '(^|[^A-Za-z0-9_-])gh[[:space:]]+repo[[:space:]]+(clone|view|fork)[[:space:]]+[A-Za-z0-9_.-]+/' | sed -E 's#.*[[:space:]]##; s#/$##'
             printf '%s' "$cmd" | grep -Eo '(^|[[:space:]/])repos/[A-Za-z0-9_.-]+/' | sed -E 's#.*repos/##; s#/$##'
             printf '%s' "$cmd" | grep -Eo '(^|[^A-Za-z0-9_-])gh[[:space:]]+api[[:space:]]+(-[^[:space:]]+[[:space:]]+)*/?(orgs|users)/[A-Za-z0-9_.-]+' | sed -E 's#.*(orgs|users)/##')
  fi
}

case "$tool" in
  WebSearch)
    block "web search is disabled on the clean side"
    ;;
  WebFetch)
    url="$(printf '%s' "$input" | sed -n 's/.*"url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
    host="$(printf '%s' "$url" | sed -n 's#^[A-Za-z][A-Za-z0-9+.-]*://\([^/:?#]*\).*#\1#p' | tr 'A-Z' 'a-z')"
    for allowed in "${WEBFETCH_ALLOW[@]}"; do
      [ "$host" = "$allowed" ] && exit 0
    done
    block "WebFetch to '${host:-unknown host}' is not on the documentation allowlist"
    ;;
  Bash|Read|Grep|Glob)
    tool_input="$(printf '%s' "$input" | sed -n 's/.*"tool_input"[[:space:]]*:\(.*\)/\1/p')"
    if printf '%s' "$tool_input" | grep -Eiq "$TAINT_PATTERN"; then
      match="$(printf '%s' "$tool_input" | grep -Eio "$TAINT_PATTERN" | head -n1)"
      block "reference to tainted material ('$match')"
    fi
    if [ "$tool" = Bash ]; then
      cmd="$(printf '%s' "$input" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\(\([^"\\]\|\\.\)*\)".*/\1/p' | head -n1)"
      check_bash_network "$cmd"
    fi
    ;;
esac

exit 0
