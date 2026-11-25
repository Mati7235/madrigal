# Chip

32px filter chip for search filters, library tags and dialog options.

- **Redesign proposal:** no outline. Unselected chips are filled `surface-container-highest` with `on-surface` labels; the selected chip turns `secondary` / `on-secondary`, gains a 14px check and tightens its corners from 16 to 10 (200ms `easing-emphasized`).
- Selection reads by fill, check and shape. `secondary` keeps chips apart from the `primary` SegmentedButton pill and from `primary-container` playing rows.
- Label `label-large`; hover 0.08 layer; keyboard focus 2px inset ring. Rows of chips scroll horizontally, never wrap.
- Contrast: `on-secondary` on `secondary` 6.5:1 (Light) / 11.9:1 (Dark); `on-surface` on `surface-container-highest` 14:1 / 13:1.
- Slint: `M3Chip { text, selected, selected-fill, selected-content, unselected-content, unselected-line }` in `cards.slint`; `unselected-line` goes, `unselected-fill` arrives.
