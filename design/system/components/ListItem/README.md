# ListItem

54px track row used by every song list: search, library, queue, downloads.

- 36px cover, title `title-small` 14/500, subtitle and duration `label-small` 11px `on-surface-variant`.
- Hover `surface-container`; playing row `primary-container` with `on-primary-container` text and the animated PlayingIndicator (a `primary` title there is 4.20:1 in Light, under the floor).
- **Proposal:** the trailing "More actions" button shows only on hover, keyboard focus and the playing row (second row shows hover), so long lists read as text, not a column of icons. It stays in the tab order and keeps its accessible label; right click opens the same TrackMenu.
- Download state: 120×6 progress bar (`surface-variant` track, `primary` fill).
- Slint: `M3ListItem { title, subtitle, duration, active, progress, menu, row-bg, active-bg, accent }` in `rows.slint`.
