# Reglas de trabajo

- Español para hablar y para los commits (imperativos, ≤50 caracteres, sin punto final).
- Inglés para el código (identificadores, tipos, rutas).
- Sin comentarios ni docstrings; excepción: un comentario de una línea para un workaround no deducible.
- Documentación mínima y sin prosa; el "por qué" va en `docs/`, nunca en el código.
- `cargo xtask checks` es la única puerta de calidad (contrato en `docs/TESTING.md`).
- Checks autocontenidos: aíslan XDG en un directorio temporal, sin red salvo grupo `live`.
- Fixtures con datos reales, nunca inventados.
- Secretos nunca en el repo (ni tokens, ni cookies, ni credenciales).
- Nada de Qt ni Python en el producto: Rust en todo el workspace.
- Mapa de documentos: `docs/README.md`.
