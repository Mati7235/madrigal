# DetailScreen

Album detail at 1280×720, opened from Home. Stacks over any page (`detail-stack`), so the sidebar keeps the page it came from.

- Back text button, DetailHeader, then a numbered track list with a hairline column header (#, Title, duration).
- The playing track swaps its number for the PlayingIndicator and takes `primary-container`.
- Loading, empty and error states use `common.loading`, `detail.empty`, `detail.error` with Retry.
