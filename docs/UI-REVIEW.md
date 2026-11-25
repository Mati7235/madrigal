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
| 11 | Error and empty states in reference | Done: `err-download`, `err-lyrics`, `err-offline`, `err-unavailable` |
| 12 | Contrast across themes and tints | Open |
| 13 | Focus rings on every control | Open |

## New findings

14. **The offline banner is a solid dark red bar** (`#93000a` fill, white text). The spec calls for a tonal `error-container` fill with `on-error-container` text. Match the spec, or document why the banner is stronger.
15. **Home still uses the Downloads arc tile**, which conflicts with the error language used in ErrorState. Same as item 5.

## Next steps

- Items 5, 6, 7 and 14 are layout and colour fixes in the prototype. Do them together, then regenerate the affected Home and Downloads PNGs.
- Items 4, 9 and 10 need a decision from the user before changing copy or colour.
- Items 12 and 13 are measurement work and can be done in one pass.
