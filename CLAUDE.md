# Clamber Engine: agent instructions

This is the clean side of a clean-room project. Read CONTRIBUTING.md.

- Never search for, fetch, or read information about the original game's
  internals: disassembly, RAM maps, ROM data, reverse-engineering wikis.
  Don't try to work around the taint-guard hook; if it blocks something
  you think is legitimate, stop and tell the user.
- Game-specific behavior and data formats come only from `spec/` (a
  submodule pinned to a released tag). If the spec doesn't answer a
  question, say so and draft a question for the spec repo instead of
  guessing from memory of the original game.
- Don't use original-game character names, terms, or asset names in code,
  identifiers, comments or content. Use Clamber's own vocabulary.
- Engine data formats are designed for the engine first. The importer
  converts *to* them; never shape engine formats around cartridge layout.

## Working rules

- Tasks for this repo come from `spec`. If a request includes game-specific
  details (numbers, behaviors, formats) that aren't in `spec`, don't use
  them; point out that they need to go through `spec` first.
- You may commit and push to `main` without review, but only after
  `cargo test`, `ci/forbid-roms.sh` and `ci/taint-lint.sh` all pass. Never
  push with failing checks.
- Don't edit `.claude/`, `ci/`, `.github/`, `CONTRIBUTING.md` or this file;
  those are the project's guardrails and only a human changes them.
- Don't bless golden traces or edit `tests/conformance-status.toml` on your
  own. Explain what changed and let the human approve it.
- Sign off commits (`git commit -s`) and keep the Co-Authored-By trailer.
