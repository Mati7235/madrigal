# Tabs

Library's primary tabs and the tag chip row beneath them.

- **Proposal:** M3 primary tabs: 48px, `label-large`, active tab `primary` text with a 3px rounded `primary` indicator inset 12px, 1px `outline-variant` baseline. Today the library tabs are pills; pills stay for secondary switches (queue tabs, player tabs).
- Tabs: Library, Songs, Albums, Artists, Playlists, History, Stats.
- Tag row: filter chips with an "All" chip first, a text-style "New tag" chip, and Edit / Done on the right; the row scrolls horizontally, never wraps.
- Slint: `LibraryPage` in `pages/library/library.slint`.
