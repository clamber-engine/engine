# Contributing to Clamber

Clamber is developed under a **clean-room process**. This repository is the
*clean side*: everything here must be written without knowledge of the
original game's internals.

## The rule

Clean-side contributors must not have studied:

- the original game's code, disassembly or decompilation
- RAM maps, memory addresses, or ROM contents (hex dumps, extracted data)
- community reverse-engineering documentation of the game

Playing the game, watching videos of it, and reading released versions of
`spec` are all fine. They're encouraged, since that's how we judge feel.

Behavior that has to match the original comes from `spec` (pinned at a
released tag), never from memory of internals or from outside sources.
If `spec` is unclear, open an issue on the `spec` repo. Don't ask people
on the research side directly.

## Sides are permanent

Each contributor is on one side. Someone who has done research-side work
cannot contribute to this repository. If you're unsure whether something
you've read counts, ask before contributing.

## Every contribution

- Sign off your commits (`git commit -s`). Your sign-off includes the
  clean-room declaration in the pull request template.
- By contributing, you agree to license your code under the MIT License and
  your original content (in `content/`) under CC BY 4.0, as described in
  the README. Third-party content needs a compatible licence and an entry in
  `content/base/CREDITS.md`.
- CI rejects ROM images and flags code that looks like it was derived from
  the original program (console memory addresses, 65816 assembly).
  Flagged lines get human review; a flag is not an accusation.

## AI coding agents

Agents may help with general Rust and engine work. They may **not** be used
to research the original game, or to produce game-specific constants or
importer format details that don't come from `spec`.

The repo ships `.claude/settings.json` and a taint-guard hook that block
web search, restrict web fetches to documentation sites, and block access
to ROM files and research material. These are tripwires. The real
boundary is your environment:

- Don't keep research checkouts or ROM files on the machine or VM where
  you do clean-side work.
- Authenticate `gh`/git there with a fine-grained token scoped only to this
  org's public repositories.
- Don't give clean-side agents browser tools that use your logged-in
  sessions.
