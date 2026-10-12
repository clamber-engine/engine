# Testing

Clamber's tests have five layers. All of them run with `cargo test`, and
CI runs them on every pull request.

| Layer | Compares | Strictness | Lives in |
|---|---|---|---|
| 1. Unit | Functions against hand-written expectations | Exact | Next to the code |
| 2. Format | Scenario and trace parsers against fixtures | Exact | `crates/trace/tests/` |
| 3. Determinism | An engine run against a second run of itself | Byte-identical | `crates/sim/tests/determinism.rs` |
| 4. Golden | Engine traces against the engine's own committed traces | Byte-identical | `tests/golden/` |
| 5. Conformance | Engine traces against reference traces from `spec` | Per-metric tolerances | `tests/conformance.rs` |

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

For every scenario, the engine's own trace is committed to
`tests/golden/<id>.trace`. Any change to engine behavior changes a golden
trace, so behavior never changes silently, even for scenarios that have no
reference trace yet.

When a change is intentional, re-record ("bless") the traces and commit
them with the change:

```
CLAMBER_BLESS=1 cargo test --test golden
git diff tests/golden/    # review exactly what moved
```

## 5. Conformance against the original

For every scenario in `spec/scenarios/` that has a reference trace in
`spec/traces/reference/`, the harness:

1. builds the scenario's test level from `content/test/<id>.level`
2. runs the engine headlessly with the scenario's inputs
3. computes every metric in the scenario's `[metrics]` section for both
   traces (definitions in `spec/traces/METRICS.md`)
4. reports pass or fail per metric, with the first divergence for curves

Each scenario shows up as its own named test, so `cargo test run-jump`
runs one.

### The ratchet

Conformance will be incomplete for a long time, so CI tracks it with
`tests/conformance-status.toml`:

```toml
# Metrics expected to fail today. Everything not listed must pass.
[run-jump-flat]
expected_failures = ["airtime", "curve y,vy from jump_start"]
```

- A metric that is **not** listed and fails → CI fails (a regression).
- A metric that **is** listed and now passes → CI fails too, asking you to
  remove it from the list. Progress gets locked in and can't silently slip
  back.

`cargo run -p clamber-trace -- report` prints a conformance summary
(passing metrics per scenario) for tracking progress.

## Tools

`clamber-trace` (in `crates/trace`) is a small CLI built on the same code
the tests use:

```
clamber-trace record <scenario.scn> -o out.trace   # run the engine headlessly
clamber-trace diff <reference.trace> <candidate.trace> --scenario <scenario.scn>
clamber-trace report                                # conformance summary
```

The research harness writes the same trace format from the emulator, so
`diff` also works for research-side sanity checks.

## Adding a scenario

1. A scenario is added to `spec` (either side can propose one; it goes
   through gatekeeper review like everything else).
2. The research side records its reference trace into
   `spec/traces/reference/`.
3. The engine side builds `content/test/<id>.level` to satisfy the
   scenario's `requires:` line, bumps the `spec` submodule, records a
   golden trace, and lists any failing metrics in
   `conformance-status.toml`.
