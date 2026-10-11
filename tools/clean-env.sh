#!/usr/bin/env bash
# Set up and verify a clean-side Clamber environment.
#
#   tools/clean-env.sh setup   Log gh and git in with a clean-side token
#                              (pasted on stdin, not echoed) and set the git
#                              identity to that account.
#   tools/clean-env.sh check   Verify this environment can reach the clean
#                              repos and nothing on the research side.
#
# Run it in the clean environment (separate WSL distro or VM), never in the
# research one. `check` exits non-zero if anything fails.

set -u

ENGINE_REPO='clamber-engine/engine'
SPEC_REPO='clamber-engine/spec'
RESEARCH_REPO='clamber-research/research'

pass=0; warn=0; fail=0
ok()   { printf '  \033[32mPASS\033[0m %s\n' "$1"; pass=$((pass + 1)); }
note() { printf '  \033[33mWARN\033[0m %s\n' "$1"; warn=$((warn + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m %s\n' "$1"; fail=$((fail + 1)); }

setup() {
  command -v gh >/dev/null || { echo "gh is not installed" >&2; exit 1; }

  printf 'Paste the clean-side fine-grained token, then press Enter: '
  read -rs token; echo
  [ -n "$token" ] || { echo "No token given" >&2; exit 1; }

  printf '%s\n' "$token" | gh auth login --hostname github.com --git-protocol https --with-token || exit 1
  unset token
  gh auth setup-git

  login="$(gh api user --jq .login)" || exit 1
  id="$(gh api user --jq .id)"
  git config --global user.name "$login"
  git config --global user.email "${id}+${login}@users.noreply.github.com"

  echo "Logged in as $login; git identity set to ${id}+${login}@users.noreply.github.com"
  echo "Now run: $0 check"
}

check() {
  echo "GitHub access"
  if ! login="$(gh api user --jq .login 2>/dev/null)"; then
    bad "gh is not logged in"
  else
    echo "  (logged in as $login)"

    if [ "$(gh api "repos/$ENGINE_REPO" --jq .permissions.push 2>/dev/null)" = "true" ]; then
      ok "can push to $ENGINE_REPO"
    else
      bad "cannot push to $ENGINE_REPO"
    fi

    case "$(gh api "repos/$SPEC_REPO" --jq .permissions.push 2>/dev/null)" in
      false) ok "can read but not push to $SPEC_REPO" ;;
      true)  note "can push to $SPEC_REPO; clean-side accounts should only read it" ;;
      *)     bad "cannot read $SPEC_REPO" ;;
    esac

    if gh api "repos/$RESEARCH_REPO" >/dev/null 2>&1; then
      bad "CAN read $RESEARCH_REPO; this token or account must not reach the research org"
    else
      ok "cannot reach $RESEARCH_REPO"
    fi
  fi

  echo "Git identity"
  name="$(git config --global user.name || true)"
  if [ -n "${login:-}" ] && [ "$name" = "$login" ]; then
    ok "git user.name matches the GitHub account ($name)"
  else
    note "git user.name is '${name:-unset}', expected '${login:-the clean account}'"
  fi

  echo "Filesystem"
  roms="$(find "$HOME" -xdev \( -iname '*.sfc' -o -iname '*.smc' -o -iname '*.swc' -o -iname '*.fig' -o -iname '*.spc' \) -print 2>/dev/null | head -n 5)"
  if [ -z "$roms" ]; then ok "no ROM images under $HOME"; else bad "ROM images found:"; printf '       %s\n' $roms; fi

  research_clones=""
  while IFS= read -r gitdir; do
    if git --git-dir="$gitdir" remote -v 2>/dev/null | grep -q 'clamber-research'; then
      research_clones="$research_clones ${gitdir%/.git}"
    fi
  done < <(find "$HOME" -xdev -name .git -type d -prune 2>/dev/null)
  if [ -z "$research_clones" ]; then ok "no research checkouts under $HOME"; else bad "research checkouts found:$research_clones"; fi

  if [ -d /mnt/c ] && [ -n "$(ls -A /mnt/c 2>/dev/null)" ]; then
    note "Windows drives are mounted at /mnt/c; disable automount in /etc/wsl.conf"
  else
    ok "Windows drives are not mounted"
  fi

  if ls "$HOME"/.ssh/id_* >/dev/null 2>&1; then
    note "SSH keys exist in ~/.ssh; make sure none can reach the research org"
  else
    ok "no SSH keys in ~/.ssh"
  fi

  echo
  echo "$pass passed, $warn warnings, $fail failed"
  [ "$fail" -eq 0 ]
}

case "${1:-}" in
  setup) setup ;;
  check) check ;;
  *) echo "usage: $0 setup|check" >&2; exit 2 ;;
esac
