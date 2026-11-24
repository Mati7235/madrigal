# Working rules

- English across the repo: commits (imperative, ≤50 chars, no trailing period), code, and docs.
- No comments or docstrings; exception: a one-line comment for a non-obvious workaround.
- Minimal documentation with no prose; the "why" lives in `docs/`, never in the code.
- `cargo xtask checks` is the only quality gate (contract in `docs/TESTING.md`).
- Checks are self-contained: they isolate XDG state in a temp directory; no network outside the `live` group.
- Fixtures use real data, never invented.
- No secrets in the repo (no tokens, cookies, or credentials).
- No Qt and no Python in the product: Rust across the workspace.
- The UI is Material 3 Expressive: tokens in `crates/madrigal-ui/ui/tokens.slint`,
  components in `ui/components/`. Direction and decisions live in `docs/UI.md`.
  The visual target is `design/reference/` (16:9 PNGs per screen and
  component, prototype) with its spec in `docs/UI-SPEC.md`. The FigCraft
  mockups in `design/` and the Python QML oracle are superseded references,
  not specs.
- Visual work is self-reviewed against the running app PNG before reporting; the
  main agent merges on a PASS report and only spot-checks one PNG per batch.
- UI work uses the targeted check loop, never the full gate.
- Builds use all cores (`CARGO_BUILD_JOBS=12`) and serialize with
  `flock /tmp/opencode/madrigal-build.lock`; concurrent builds wait.
- Waiting for the lock is one blocking command: run `flock ... cargo ...` in the
  foreground with a long timeout and read its output when it returns. Never
  background a build and poll it with sleep/grep loops. Agents that support
  background commands with a completion notice may use that instead, and stay
  idle until the notice arrives.
- Build env: `source /tmp/opencode/env-v2.sh` (shared target dir, sccache, lld).
  Every worktree uses that one target dir so dependencies compile once. Copy a
  release binary out of it before running it: another build may replace it.
- While iterating, run only the test binaries for what changed
  (`cargo test -p <crate> --test <check>`); run the touched crates in full once,
  before reporting.
- Document map: `docs/README.md`.
