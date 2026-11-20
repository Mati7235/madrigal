# Working rules

- English across the repo: commits (imperative, ≤50 chars, no trailing period), code, and docs.
- No comments or docstrings; exception: a one-line comment for a non-obvious workaround.
- Minimal documentation with no prose; the "why" lives in `docs/`, never in the code.
- `cargo xtask checks` is the only quality gate (contract in `docs/TESTING.md`).
- Checks are self-contained: they isolate XDG state in a temp directory; no network outside the `live` group.
- Fixtures use real data, never invented.
- No secrets in the repo (no tokens, cookies, or credentials).
- No Qt and no Python in the product: Rust across the workspace.
- The approved FigCraft design (`design/madrigal-ui.figcraft`, direction in
  `design/README.md`) is the reference direction for the UI, not a pixel oracle.
  Implementation choices are made for the best app result; every value traces to
  the design or to a decision recorded in `docs/UI.md`. The Python QML oracle no
  longer governs the UI.
- Visual work is self-reviewed against the FigCraft screen PNG in `design/screens/`
  before reporting; the main agent merges on a PASS report and only spot-checks one PNG per batch.
- UI work uses the targeted check loop, never the full gate. The QML sections of
  `.opencode/skills/frontend-1to1/SKILL.md` are superseded by `design/README.md`.
- Builds use all cores (`CARGO_BUILD_JOBS=12`) and serialize with
  `flock /tmp/opencode/madrigal-build.lock`; concurrent builds wait.
- Document map: `docs/README.md`.
