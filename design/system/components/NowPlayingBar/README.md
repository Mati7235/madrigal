# NowPlayingBar

The persistent player strip: a floating card in the 96px band at the bottom of the shell, always visible.

- Card inset 8px, radius 24px, `surface-container-high`, 0.5px `outline-variant` hairline.
- 48px cover, title Inter 13/700 `on-surface`, artist 11px `on-surface-variant`.
- Transport: previous, play/pause as a filled `primary` icon button that morphs from a disc (paused) to a `radius-small` square (playing), next.
- Seek: 4px `surface-container-highest` track and a 4×20 `primary` handle; the played part waves while playing (the same fixed-wavelength, clipped wave as the full player) and is a flat `primary` line when paused. Volume: 4px track, `primary` fill.
- Like and queue are 34×40 mini buttons (radius 17 → 8 when selected, liked = `primary` fill).
- Below 840px it goes `compact`: position, duration and volume drop. The queue drawer stops above it.
- Slint: `NowPlayingBar` in `player/bar.slint`.
