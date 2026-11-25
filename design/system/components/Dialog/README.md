# Dialog

Modal dialogs: sleep timer, tempo and pitch, equalizer, playback error, confirm and prompt.

- **Proposal:** M3 dialog shape `radius-extra-large` (28) instead of the current 12, a 24px hero icon (`secondary`, or `error` for failures), centred `headline-small` title, `body-medium` supporting text.
- `surface-container-high`, `shadow-menu`, over a `scrim`. Actions trail right: text button to dismiss, one filled or tonal confirm.
- Options as chips (sleep: 15 / 30 / 45 / 60 min, End of track). Esc closes; focus stays inside.
