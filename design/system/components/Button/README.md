# Button

Common button in five emphasis levels; the caller supplies the colours from palette roles.

- **Filled** `primary`/`on-primary`: the one main action of a view (Play, Continue).
- **Tonal** `secondary-container`/`on-secondary-container`: strong secondary (Shuffle, Back to current line).
- **Outlined** `outline-variant` border, `on-surface-variant` label; **Text** label only; **Elevated** `surface-container-low` + `primary`.
- Sizes: `small` 40px (radius 12 → 8 pressed, 16px padding), `medium` 56px (16 → 12, 24px padding). Label `label-large`, elided.
- Disabled: `on-surface` at 0.12 fill, 0.38 label. Focus: 2px ring in the content colour, inset 2px.
- Slint: `M3Button { text, size, enabled, fill, content, line, state-on-surface, clicked }` in `buttons.slint`.
