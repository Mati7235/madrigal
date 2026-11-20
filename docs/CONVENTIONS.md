# Conventions

## Workspace layout

```text
Cargo.toml          # workspace: members = ["crates/*", "xtask"]
checks.toml         # check registry: name -> group
crates/
  madrigal-<area>/  # one crate per area (player, library, lyrics, ...)
    tests/checks/   # integration checks: check_<name>.rs
xtask/              # `cargo xtask checks` runner
docs/
```

## Naming

- Product crates: lowercase `madrigal-<area>`.
- Checks: file `check_<name>.rs`, test target `check_<name>`,
  `[[check]]` row with that `name` in `checks.toml`.

## Commits

- Format: `type(scope): subject`. Omit `(scope)` when the change spans crates.
  Scope is the crate name without `madrigal-`, e.g. `ui`, `player`.
- Types: `feat`, `fix`, `perf`, `build`, `ci`, `docs`, `refactor`, `style`, `test`.
- Subject: imperative mood (`add`, not `added`), lowercase after the type,
  no trailing period, at most 50 characters including the prefix.
- One commit, one change. If the subject needs more than 50 characters or the
  commit does two things, split it.
- Body (optional): blank line after the subject, lines wrapped at 72, explains
  why, not what.
- Issue references go in the body: `closes #N`, `fixes #N`, `resolves #N`.
- Language: English.

Good: `feat(player): add queue drawer`, `fix: keep selection after reload`.
Bad: `Added queue drawer.`, `fix: Fixed the topbar and changed the colors`.

## Merge rules

- One task = one worktree (see `.worktrees/`, git-ignored).
- Only the coordinator merges; worktrees propose, they don't integrate.
- Every merge runs `cargo xtask checks` green before landing.
- Builds in worktrees run with `CARGO_BUILD_JOBS=2`: the machine bottleneck is
  CPU (12 cores shared by many parallel tasks), not RAM.
