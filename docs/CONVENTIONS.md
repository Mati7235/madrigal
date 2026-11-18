# Convenciones

## Estructura del workspace

```text
Cargo.toml          # workspace: members = ["crates/*", "xtask"]
checks.toml         # registro de checks: nombre -> grupo
crates/
  madrigal-<area>/  # un crate por área (player, library, lyrics, ...)
    tests/checks/   # checks de integración: check_<nombre>.rs
xtask/              # runner `cargo xtask checks`
docs/
```

## Nombres

- Crates del producto: `madrigal-<area>` en minúsculas.
- Checks: archivo `check_<nombre>.rs`, target de test `check_<nombre>`,
  fila `[[check]]` con ese `name` en `checks.toml`.

## Reglas de merge

- Una tarea = un worktree (ver `.worktrees/`, ignorado por git).
- Mergea sólo el coordinador; los worktrees proponen, no integran.
- Cada merge corre `cargo xtask checks` en verde antes de integrar.
