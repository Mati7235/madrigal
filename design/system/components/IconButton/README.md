# IconButton

40px icon-only button; round at rest, morphs to a `radius-small` square when `selected`.

- Icon 20px, tinted with `content` (or `icon-color`). Hover layer 0.08 of content.
- Transport (previous, play/pause, next), toggles (shuffle, repeat, like), row "More actions".
- Toggled-on state: `selected: true` plus a tonal fill, or the filled icon variant (`heart-filled`).
- Always pass an accessible label (`action.more`, `player.next`...).
- Slint: `M3IconButton { icon, selected, fill, content, line, clicked }`.
