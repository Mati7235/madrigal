# UI review

Review of the Material 3 Expressive reference (`design/reference/`) and the design system (`design/system/`). Method: the impeccable mechanical detector over `prototype.html` and every component preview, then a visual pass over the 1280×720 screens (home, full player, settings, downloads). Single pass, not the two-agent critique.

## Detector

Three findings, all in `prototype.html`:

- `gradient-text` on the karaoke fill (`.lyr3 .on .lw span.now`). False positive: the gradient shows sung progress, it is not decoration.
- `marquee` on `.dlr .pg.q i` and `.lin.ind i`. False positive: indeterminate progress bars, not content that scrolls.

No findings in the design system previews.

## Priority 1: fix before the agent builds

1. **The reference screenshots of the full player are stale.** `screens/1280x720/full-player-*.png` still show the old version: a glow under the cover, "Synced · sample", the save icon for sharing, "1.00×" and "Flat" without labels, and the Up next card. Regenerate the PNGs from the committed `prototype.html` after the chosen option 3 is applied.
2. **The bottom bar stays visible under the full player** in the reference. It duplicates the transport. Hide `NowPlayingBar` while the full player is open (the prototype already does this in the local version).
3. **Decorative glow on the cover.** The tinted box-shadow under the cover is the "brillo" the user rejected. Remove it in the reference and in `FullPlayer/preview.html`.

## Priority 2: consistency

4. **Accent colour does not carry across themes.** Dark uses lavender; Light uses saturated blue for the selected segment, the play button and the nav pill. Decide whether Light should use a lavender-family primary, and write the decision in `design/system/README.md`.
5. **The Downloads tile on Home looks like an error.** Its rose fill and arc shape read as failure next to the new ErrorState. Use a `secondary-container` or `tertiary-container` fill, or drop the arc morph on that tile.
6. **Downloads rows have a dead middle.** The title column and the status column sit far apart, with empty space between them. Put the progress bar and status next to the title, or reduce the row width.
7. **Quick picks crowd their durations.** "4:21" sits right against the artist line. Give the duration its own right-aligned column.
8. **Prototype chrome leaks into the reference.** "Prototype with sample data. Only Appearance is wired up." shows on the Settings screen. Keep it out of the spec screens or mark it as a note in `UI-SPEC.md`.

## Priority 3: copy and states

9. **Technical labels.** Replace "Synced · sample" with "Synced" (done in option 3 locally), "Buffer", "Route", "Codec" with plain words where the user does not need the term.
10. **Lyrics size shows raw units** ("22 px"). Show a step name or hide the unit.
11. **Error and empty states** exist in the design system (`ErrorState`, `EmptyState`) but no reference screen uses them yet. Add one failed download and one offline screen.

## Priority 4: accessibility

12. **Contrast audit across every theme and cover tint** is still open. Measured so far: full player and its components. Not yet measured: Home cards, Downloads status text, Settings secondary text on Pure Black.
13. **Focus rings.** Check that every control in the reference shows the 2px inset ring when focused, including the chips and the segmented buttons.

## Next steps

- Items 1–3 unblock the agent. Do them first.
- Items 4–8 need a decision from the user on the accent colour and the Downloads layout; ask before changing them.
- Items 9–13 can be done in one pass once 1–3 are in.
