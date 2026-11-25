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
| 4 | Accent colour in Light | Open: Light primary is still `#0b6bcb`, Dark is `#cfbcff` |
| 5 | Downloads tile on Home reads as error | Open: still a rose arc tile on Home |
| 6 | Downloads rows have a dead middle | Open |
| 7 | Quick picks crowd their durations | Open in the dark home screen |
| 8 | Prototype note on Settings | Done |
| 9 | Technical labels in Details | Open: "Codec", "Route", "Buffer" still in the Details tab |
| 10 | Lyrics size shows "22 px" | Open |
| 11 | Error and empty states in reference | Done: `err-download`, `err-lyrics`, `err-offline`, `err-unavailable`. The offline banner was lost in merge `8397095` and restored in this commit |
| 12 | Contrast across themes and tints | Partly done: 0 text pairs below threshold on the 6 main screens in Light, Dark and Pure Black. Tints not yet measured. Closest pair: the inactive lyric "rain" at 3.9:1 (Light, 42px bold, needs 3.0) |
| 13 | Focus rings on every control | Partly done: the first 40 Tab stops on Home show a visible ring in all three themes. Other screens not yet checked |

## New findings

14. **The offline banner is a solid dark red bar.** It follows the spec: it uses `error-container`, which in Dark is `#93000a`. The colour is strong. Decide whether that token value should change (a lighter or less saturated `error-container` for Dark), since it sets every error surface.
15. **Home still uses the Downloads arc tile**, which conflicts with the error language used in ErrorState. Same as item 5.

## Next steps

- Items 5, 6, 7 and 14 are layout and colour fixes in the prototype. Do them together, then regenerate the affected Home and Downloads PNGs.
- Items 4, 9 and 10 need a decision from the user before changing copy or colour.
- Items 12 and 13: measure the cover tints and the other screens' Tab stops.
