# SideNav

Main navigation: a 244px sidebar that collapses to an 80px icon rail below 1100px of window width.

- `surface-container-low` panel, 16px padding, 6px between items.
- Items are 48px pills: 22px icon, 12px gap, `label-large` label. Active: `secondary-container` / `on-secondary-container`; rest: `on-surface-variant`.
- Rail: icons centred, labels move to a tooltip (`tip-text`). Focus: 2px `primary` ring inset 2px.
- Pages: Home, Search, Explore, Library, Downloads, Settings; keys `1`..`6`, `[` / `]` cycle.
- Icons: `house`, `search`, `compass`, `library`, `download`, `settings`. **Fix:** `sidebar.slint` uses `arrow-down` for Downloads; use `download` so the nav matches every other downloads entry point (Home shortcut, track menu, detail header).
- Slint: `SideNav { palette, page, expanded }` in `shell/sidebar.slint`.
