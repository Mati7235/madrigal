# SegmentedButton

40px pill group for mutually exclusive views (queue tabs, theme mode).

- **Redesign proposal:** an M3 Expressive connected button group. No outline: each segment is its own `surface-container-highest` button, 2px apart; inner corners 8px, outer corners pill.
- The selected segment becomes a full pill in `primary` / `on-primary` with a check, morphing its corners in 200ms `easing-emphasized`. Selection reads by fill, shape and check, never by colour alone.
- Labels `label-large`; hover adds the 0.08 state layer; keyboard focus draws a 2px inset ring.
- Used for the queue tabs, theme mode, equalizer presets and the diagnostics level filter.
- Slint: `M3SegmentedButton` in `selection.slint`.
