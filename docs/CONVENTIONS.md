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

## Merge rules

- One task = one worktree (see `.worktrees/`, git-ignored).
- Only the coordinator merges; worktrees propose, they don't integrate.
- Every merge runs `cargo xtask checks` green before landing.
- Builds in worktrees run with `CARGO_BUILD_JOBS=2`: the machine bottleneck is
  CPU (12 cores shared by many parallel tasks), not RAM.
