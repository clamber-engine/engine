# Clamber Engine: agent instructions

This is the clean side of a clean-room project. Read CONTRIBUTING.md.

- Never search for, fetch, or read information about the original game's
  internals: disassembly, RAM maps, ROM data, reverse-engineering wikis.
  Don't try to work around the taint-guard hook; if it blocks something
  you think is legitimate, open a `needs-director` issue and move on.
- Game-specific behavior and data formats come only from `spec/` (a
  submodule pinned to a released tag). If the spec doesn't answer a
  question, say so and draft a question for the spec repo instead of
  guessing from memory of the original game.
- Don't use original-game character names, terms, or asset names in code,
  identifiers, comments or content. Use Clamber's own vocabulary.
- Engine data formats are designed for the engine first. The importer
  converts *to* them; never shape engine formats around cartridge layout.

## Working rules

- Work comes from issues on this repo. Game-specific details (numbers,
  behaviors, formats) come only from `spec`. If an issue or instruction
  includes game-specific details that aren't in `spec`, don't use them;
  open a spec issue instead.
- You may commit and push to `main` without review, but only after
  `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`,
  `ci/forbid-roms.sh` and `ci/taint-lint.sh` all pass. Never push with
  failing checks. After pushing, check that CI passed (`gh run list`); if it
  failed, fixing it comes before anything else.
- Don't edit `.claude/`, `ci/`, `.github/`, `CONTRIBUTING.md` or this file;
  those are the project's guardrails. If one needs changing, open a
  `needs-director` issue.
- Bless golden traces only when the behavior change is intended, in its own
  commit whose message explains what changed and why.
- In `tests/conformance-status.toml`, remove expected failures as soon as
  they pass. Add one only together with an open issue that tracks fixing
  it, linked in a comment next to the entry.
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
- **Need a spec answer?** Open an issue on `clamber-engine/spec` following
  the Question, Correction or Research request template, and **add the
  labels yourself** (`--label question,needs-research`, or
  `correction,needs-research`, or just `needs-research`); `gh` doesn't apply
  template labels, and unlabeled issues are invisible to the research
  side. Link it from the
  engine issue, label the engine issue `blocked-on-spec`, and work on
  something else. When a new spec release answers it, bump the `spec`
  submodule to that tag and continue.
- **Need a decision** about scope, design direction, or anything legal?
  Label the issue `needs-director`, lay out the options in a comment, and
  move on.
- Only use `spec` at released tags. Don't read the spec repo's branches or
  pull requests.
