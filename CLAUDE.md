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

## Working autonomously

You work without a human watching. The director sets direction through
issues and only reads issues labeled `needs-director`. Process details are
in `spec/COMMUNICATION.md`.

- **Pick work** from open issues labeled `ready` (`gh issue list --label ready`).
  The pinned "Roadmap" issue gives the order. Claim an issue by assigning
  yourself and adding `in-progress`; skip issues someone else has claimed.
- **Finish** by pushing a commit that says `Fixes #N`. Split large issues
  into smaller ones and link them.
- **Need a spec answer?** Open an issue on `clamber-engine/spec` with the
  Question, Correction or Research request template, link it from the
  engine issue, label the engine issue `blocked-on-spec`, and work on
  something else. When a new spec release answers it, bump the `spec`
  submodule to that tag and continue.
- **Need a decision** about scope, design direction, or anything legal?
  Label the issue `needs-director`, lay out the options in a comment, and
  move on.
- Only use `spec` at released tags. Don't read the spec repo's branches or
  pull requests.
