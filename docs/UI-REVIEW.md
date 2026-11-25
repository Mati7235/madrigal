# UI review

Review of the Material 3 Expressive reference (`design/reference/`) and the design system (`design/system/`). Method: the impeccable mechanical detector over `prototype.html` and every component preview, then a visual pass over the 1280×720 dark screens (home, downloads, full player, offline error).

## Detector

Three findings, all in `prototype.html`. All are false positives:

- `gradient-text` on the karaoke fill. The gradient shows sung progress, it is not decoration.
- `marquee` on `.dlr .pg.q i` and `.lin.ind i`. Indeterminate progress bars, not scrolling content.

No findings in the design system previews.

## Status of the earlier review

| # | Item | Status |
|---|------|--------|
| 1 | Full player reference PNGs stale | Done: full player matches the chosen layout |
| 2 | Bottom bar under the full player | Done |
| 3 | Glow and bounce on cover and play | Done |
| 4 | Accent colour in Light | Done: Light moves to the same violet seed as Dark (`primary` `#6750a4`, violet-grey surfaces). One seed across the three themes. Updated in `tokens.slint`, both `tokens.json`, the prototype, the app icon, the docs and the Light PNGs (1920×1080 and 1280×720, error states included) |
| 5 | Downloads tile on Home reads as error | Done: tonal `secondary-container` tile |
| 6 | Downloads rows have a dead middle | Done |
| 7 | Quick picks crowd their durations | Done: durations in their own column, hover actions hidden in Quick picks so artist and album fit |
| 8 | Prototype note on Settings | Done |
| 9 | Technical labels in Details | Done: "Format", "Playing from", "Buffered" |
| 10 | Lyrics size shows "22 px" | Done: Small, Medium, Large, Extra large |
| 11 | Error and empty states in reference | Done: `err-download`, `err-lyrics`, `err-offline`, `err-unavailable`. The offline banner was lost in merge `8397095` and restored in this commit |
| 12 | Contrast across themes and tints | Done: 0 text pairs below threshold on Home and the full player with all 6 cover tints, and on the 6 main screens, in Light, Dark and Pure Black. Closest pair: the inactive lyric "rain" at 3.9:1 (Light, 42px bold, needs 3.0) |
| 13 | Focus rings on every control | Done: Tab through Home, Search, Library, Downloads, Settings and the full player (up to 80 stops each) in all three themes. Every stop shows a ring. Text fields ring their `.tf .f` wrapper (2px primary, inset) |

## New findings

14. **The offline banner is a solid dark red bar.** Decided: keep. It uses `error-container` (`#93000a` in Dark) with `on-error-container` text, the standard Material 3 pairing, and it passes contrast. Every error surface shares it, so changing it is a token change, not a banner fix.
15. **Home Downloads tile**: fixed, see item 5.

## Next steps

- Component sheets in `design/reference/components/`: the Light band of all 46 was recoloured from the old blue to the violet tokens by palette mapping, since the HTML sources were lost. Dark and Pure Black bands are untouched. A rebuild from the prototype styles would give exact values.
