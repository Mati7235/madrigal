# SettingsRow

One settings row, driven by the `SettingsRow` model; its `kind` picks the trailing control.

- Kinds: `switch` (M3Switch), `choice` with up to 4 options (SegmentedButton) or more (Select pill, menu opens toward the row's end edge), `slider` (value label on the right), `text` (dense 40px pill TextField, the row title is its label), `apikey` (masked dense pill with Show / Hide).
- Title `body-large`, supporting line `body-medium` `on-surface-variant`; rows at least 64px.
- **Proposal:** group rows into `surface-container-low` cards at `radius-large` with a `primary` group label and 1px `outline-variant` dividers; advanced rows stay hidden behind "Show advanced".
- Slint: `SectionView` in `pages/settings/section.slint`, wired 1:1 to `madrigal-settings` by `settings_section`.
