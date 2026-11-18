# Madrigal — reproductor de música de escritorio

Reproduce tu biblioteca local y YouTube Music (streaming y descargas),
con letras sincronizadas, scrobbling y control MPRIS.

## Requisitos

- Rust ≥ 1.100 (`rustup toolchain install stable`)
- libmpv (reproducción)
- yt-dlp (streams y descargas de YouTube)
- ffmpeg (conversión y metadatos)
- deno o node (provider PO para streaming de YouTube Music)

## Checks

`cargo xtask checks` es la única puerta de calidad.

```sh
cargo xtask checks                        # todos los grupos
cargo xtask checks --group offline        # offline | native | live | visual
cargo xtask checks --only queue           # un solo check
cargo xtask checks --max-fails 1          # corta en la primera falla
```

Detalle del contrato en `docs/TESTING.md`.

## Licencia

GPL-3.0-or-later, ver `LICENSE`.
