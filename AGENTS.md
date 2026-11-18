# Working rules

- English across the repo: commits (imperative, ≤50 chars, no trailing period), code, and docs.
- No comments or docstrings; exception: a one-line comment for a non-obvious workaround.
- Minimal documentation with no prose; the "why" lives in `docs/`, never in the code.
- `cargo xtask checks` is the only quality gate (contract in `docs/TESTING.md`).
- Checks are self-contained: they isolate XDG state in a temp directory; no network outside the `live` group.
- Fixtures use real data, never invented.
- No secrets in the repo (no tokens, cookies, or credentials).
- No Qt and no Python in the product: Rust across the workspace.
- Document map: `docs/README.md`.
