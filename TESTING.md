# Testing

Clamber's tests have five layers. All of them run with `cargo test`, and
CI runs them on every pull request.

| Layer | Compares | Strictness | Lives in |
|---|---|---|---|
| 1. Unit | Functions against hand-written expectations | Exact | Next to the code |
| 2. Format | Scenario and trace parsers against fixtures | Exact | `crates/trace/tests/` |
| 3. Determinism | An engine run against a second run of itself | Byte-identical | `crates/sim/tests/determinism.rs` |
| 4. Golden | Engine traces against the engine's own committed traces | Byte-identical | `tests/golden/`, checked by `crates/conformance/tests/golden.rs` |
| 5. Conformance | Engine traces against reference traces from `spec` | Per-metric tolerances | `crates/conformance/tests/conformance.rs` |

## 1. Unit tests

Ordinary `#[test]` functions for math, collision, input handling and so on.
Nothing special.

## 2. Format tests

The scenario and trace parsers in `crates/trace` are tested against small
fixture files, including malformed ones that must be rejected with a
useful error. These parsers are shared by the engine recorder, the diff
tool and the test harness, so they get tested on their own.

## 3. Determinism

Every scenario runs twice in the same process; the traces must be
byte-identical. CI also runs it on Linux, macOS and Windows and compares
the traces across platforms. Floating-point drift or iteration order
leaking into the simulation shows up here first.

## 4. Golden traces (self-regression)

For every scenario, and every whole-level playthrough in
`tests/playthrough/`, the engine's own trace on the `default` preset is
committed to `tests/golden/<id>.trace`. (Playthroughs for other presets,
in `tests/playthrough/<preset>/`, are only checked to reach the goal.) Any change to engine behavior changes a golden
trace, so behavior never changes silently, even for scenarios that have no
reference trace yet.

When a change is intentional, re-record ("bless") the traces and commit
them with the change:

```
CLAMBER_BLESS=1 cargo test --test golden
git diff tests/golden/    # review exactly what moved
```

## 5. Conformance against the original

For every scenario in `spec/scenarios/` and every title that has a
reference trace for it (`spec/traces/reference/<id>.trace` for `title-1`,
`spec/traces/reference/<title>/<id>.trace` for the others), the harness:

1. builds the scenario's test level from `content/test/<id>.level`, or
   from `content/test/<title>/<id>.level` when the scenario's
   `requires:` line asks something different of that title
2. runs the engine headlessly with the scenario's inputs, on the title's
   preset: `classic-1` for `title-1`, `classic-2` for `title-2`
   (`CONFORMANCE_TITLES` in `crates/conformance`). A preset says which
   title its values were measured on (`title = "title-1"`), so its traces
   carry a `# title:` line; a trace whose title differs from the
   reference's is a title mismatch and isn't compared
3. computes every metric in the scenario's `[metrics]` section for both
   traces (definitions in `spec/traces/METRICS.md`)
4. reports pass or fail per metric, with the first divergence for curves

Each scenario shows up as its own named test per title,
`conformance::<id>` for `title-1` and `conformance::<title>/<id>` for the
others, so `cargo test --test conformance run-jump` runs one. Scenarios
without a reference trace for a title show up as ignored.

### The ratchet

Conformance will be incomplete for a long time, so CI tracks it with
`tests/conformance-status.toml`:

```toml
# Metrics expected to fail today. Everything not listed must pass.
[run-jump-flat]
expected_failures = ["airtime", "curve y,vy from jump_start"]
```

Title-1 scenarios are listed by id; the others as `["title-2/<id>"]`.

- A metric that is **not** listed and fails → CI fails (a regression).
- A metric that **is** listed and now passes → CI fails too, asking you to
  remove it from the list. Progress gets locked in and can't silently slip
  back.

`cargo run --bin clamber-trace -- report` prints a conformance summary
(passing metrics per scenario) for tracking progress.

## Tools

`clamber-trace` (in `crates/conformance`, built on the formats in
`crates/trace`) is a small CLI built on the same code the tests use:

```
clamber-trace record <scenario.scn> -o out.trace   # run the engine headlessly
                                                   # (--level, --preset to override)
clamber-trace diff <reference.trace> <candidate.trace> --scenario <scenario.scn>
clamber-trace report                                # conformance summary
```

The research harness writes the same trace format from the emulator, so
`diff` also works for research-side sanity checks.

`clamber-replay` (in `crates/game`) is for differential testing against
the original, frame by frame, positions and pictures. It replays an input
recording on a level of an imported pack from the level's start, with no
window, deterministically:

```
clamber-replay --pack <dir> --inputs <file> --out <dir> [--level <id>]
               [--preset <id>] [--arrival <name>] [--restart <n>]
               [--frames 0..3600] [--every 1] [--layers]
```

- `--pack` is an imported pack's folder (`packs/<id>`, whose `current`
  file names the import in use; or that import's folder, holding
  `manifest.toml`); `--level` is a file name in its `levels/` folder
  without `.level`, the first one by default. `--arrival` starts at one
  of the level's arrivals instead of its start. `--restart` comes back at
  one of its restart points (checkpoints and kegs) instead, as after a
  lost life with it reached: frame 0 is the step the player comes back
  on, inside the keg for a keg, and the trace's positions are from the
  restart point. It's the restart point's entity id, or its number among
  the level's, in their order from 0. The rest of the level is as at its
  start. The preset defaults to
  the one holding the pack's title's values (`classic-1` for `title-1`).
- `--inputs` is a scenario (`.scn`), a trace (`.trace`, its `input`
  column), or a file of held actions, one line per frame from frame 0:

  ```
  # clamber-inputs v1
  R
  RJ
  .
  ```

  Each line is the actions held that frame in the order `LRUDJASPT`, as
  in a trace's `input` column, `.` for none. Blank lines and other `#`
  lines are skipped.
- `--out` gets `replay.trace`, the trace of every frame played from frame
  0 (as `clamber-trace record` writes, positions from where the player
  started), and `<frame>.png` (six digits) for each frame in `--frames`
  (`<from>..<to>`, `to` not included), every `--every`th: the picture as
  shown after that frame's step, at the pack's own resolution, as the
  Original rendering mode draws it. Play goes from frame 0 either way, so
  a long run can be looked at in pieces with `--frames`; the trace stops
  at the last frame of the range.
- With `--layers`, each part of the game's picture is also written on its
  own, transparent where it draws nothing: `<frame>-backdrop.png`,
  `<frame>-layer-<index>-<name>.png` for each of the level's layers,
  `<frame>-sprites.png` and `<frame>-hud.png`. Menus over the game are
  only in the whole picture.

Play goes as in the game: through exits into other areas, and onto the
pause, level complete and game over screens if the inputs lead there. A
frame on which the game holds still (a pause the world asks for) has a
row with no events. The engine's tests replay a made-up pack only; real
packs and dumps from them never go into the repository.

## Adding a scenario

1. A scenario is added to `spec` (either side can propose one; it goes
   through gatekeeper review like everything else).
2. The research side records its reference trace into
   `spec/traces/reference/`.
3. The engine side builds `content/test/<id>.level` to satisfy the
   scenario's `requires:` line, bumps the `spec` submodule, records a
   golden trace, and lists any failing metrics in
   `conformance-status.toml`.
