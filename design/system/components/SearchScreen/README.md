# SearchScreen

Search at 1280×720 with results for a query. **Redesign proposal.**

- Search bar (focused, 2px `primary`) and the Online / Local source switch on one toolbar; filter chips All / Songs / Albums / Artists / Playlists beneath.
- Results: a top-result panel (`surface-container`, `radius-extra-large`, 96px art, Play + Shuffle) beside the first songs, then album and artist rails with "See all" and scroll arrows.
- Empty, error and offline states use EmptyState with `search.empty.*`, `search.error.network` and Retry.
- Below 420px of page height the hint and suggestions title hide.
