# TrackMenu

The one context menu for every song surface.

- **Redesign:** same shell as the Select menu: 304px, `surface-container-high`, `radius-large-increased` (20), 8px inner padding, `shadow-menu`.
- Header: 40px cover, title `title-medium`, artist · album `body-small`; a 1px `outline-variant` divider.
- Rows: 48px, `radius-medium` (12) inside the card, 24px `on-surface-variant` icon, 12px gap, label `body-large`. Groups: playback and library actions, then navigation, split by a divider.
- States: hover 0.08 layer; keyboard focus a 2px inset `on-surface` ring (first row); a toggled action shows its filled icon in `primary` ("Liked"); unavailable rows at 0.38 (Go to artist when unknown); "Add to playlist" carries a chevron and opens its second view.
- Keyboard: Up/Down skip unavailable rows, Enter activates, Esc closes, Left leaves the playlist view.
- Placement: anchored at the pointer or the row's More button, flips above when there is no room, never covers the 96px player bar.
- Slint: `TrackMenu` in `menu.slint`.
