# DiagnosticsPanel

Diagnostics log in Settings: level filter, latency, export.

- Level filter as a segmented button (All / Info / Warn / Error), latency p50 / p95 as `surface-container-highest` tags, Export as an outlined button.
- Log on `surface-container-lowest` with a hairline, monospace 12/20; levels coloured `secondary` / `tertiary` / `error` and always spelled out, never colour alone.
- Slint: `DiagnosticsPanel` in `pages/settings/diagnostics.slint`. The lines here are illustrative.
