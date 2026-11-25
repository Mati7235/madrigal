# FullPlayer

The full-screen player: console on the left, a pane with Lyrics / Up next / Details on the right. **Redesign**; interactive version in the prototype, PNGs in `design/reference/screens/*/full-player-*.png`.

**Colour from the artwork.** The player takes a `tint` from the cover (matugen seam; until then the album's accent). Ground = `tint` 14% into `surface`, pane = `tint` 10% into `surface-container`, cover shadow in `tint`. Background styles: Theme (flat tint), Gradient (tint fading down), Blur, Glow (radial light behind the cover).

**Header (64px).** Close (chevron-down), a "Playing from <source>" chip, Share lyrics as image, More (TrackMenu).

**Console.**
- Cover up to 480px, `radius-extra-extra-large` (48) breathing to 32 while playing; size follows the window height (`min(100%, 480, 100vh − 600)`), never below 160px.
- Title `display-small` 36/500, then artist (a link to the artist) · album · year in `body-large`; a 48px like toggle that morphs disc → `radius-large` square and fills `primary` when liked.
- Seek: wavy while playing (fixed 24px wavelength, clipped at the handle, never stretched), flat when paused; elapsed and remaining time under it in tabular figures.
- Transport by size: 48px shuffle and repeat toggles (selected = `secondary-container`, `radius-medium`, a dot under the icon; repeat cycles off / all / one), 64px previous / next on an 8% tint, play/pause 112×80 pill that tightens to a 96px `radius-extra-large` square while playing (300ms overshoot easing).
- Volume: an 8px track with a 4px handle and the value in digits.
- Up next card: the next song; opens the Up next tab.

**Pane.** Pill tab switcher (Lyrics, Up next, Details), then:
- Lyrics: a Synced chip and the −0.1 s / offset / +0.1 s control; lines in Figtree; the active line 12px larger and bold, with a karaoke sweep: sung words `on-surface`, the current word fills `primary` left to right, unsung words `on-surface` at 55% (3.8:1 minimum on every tint and theme); translation under it in `body-large`; past lines at 0.8. Tap a line to seek.
- Up next: 60px rows with jump and remove.
- Details: title, album, source, codec / bitrate / sample rate, route, loudness, buffer health.
- Tool pills at the bottom: Sleep timer (shows the time when set), speed (1.00×), equalizer preset; each opens its dialog.

**Sizes.** Two columns from 1100px wide and 700px tall; below, one scrolling column. Under 600px tall the cover shrinks to a 72px thumbnail beside the title so transport stays above the fold (640×480 reference).

**Contrast** (all tints × Light / Dark / Pure Black): title 13:1, secondary text 7.3:1, active-line unsung words 3.8:1 (large bold), sung word 4.1:1, play icon 5.3:1, cover glyph 3.2:1.

Slint: `FullPlayer` / `PlayerConsole` / `PlayerTabs` / `PlayerTools` in `player/full.slint`, `LyricsView` in `player/lyrics.slint`. The wave needs a clipped `Path` of fixed wavelength; the play morph animates width and border-radius only.
