# Testing

`cargo xtask checks` es la única puerta de calidad.

## Grupos

| Grupo   | Qué cubre                                              |
| ------- | ------------------------------------------------------ |
| offline | Lógica pura, sin red ni binarios del sistema           |
| native  | Integración con el sistema (libmpv, MPRIS, keyring)    |
| live    | Red real (YouTube, LRCLIB, SponsorBlock); opt-in       |
| visual  | Inspección de interfaz; opt-in                         |

Por defecto se corre `offline` + `native`. `live` y `visual` sólo a pedido.

## Códigos de salida

- `0`: todo PASS o SKIP, o registro vacío (`0 checks`).
- `1`: al menos un FAIL.
- `2`: uso inválido (grupo inexistente, falta valor, subcomando erróneo).

Cada check imprime `PASS|FAIL|SKIP <nombre> <grupo>`.
SKIP = el target `check_<nombre>` no existe aún o el entorno no lo soporta.
Sin `cargo` disponible el check es FAIL.

## Agregar un check

1. Crear `crates/<crate>/tests/checks/check_<nombre>.rs`.
2. Agregar la fila en `checks.toml`:
   `[[check]]` + `name = "<nombre>"` + `group = "<grupo>"`.
3. Agregar la fila a la tabla de abajo.
4. El check debe aislar XDG en un directorio temporal y no dejar estado.

## Registro

| Check | Grupo | Qué verifica |
| ----- | ----- | ------------ |
| `core_models` | offline | Roundtrip serde e invariantes de `madrigal-core` |
| `i18n_parity` | offline | Paridad es/en de madrigal-i18n: sin huérfanas ni vacías, tr() no vacío, clave inexistente da error |
