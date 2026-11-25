# QueueDrawer

The queue as a 380px side sheet that becomes full width below 800px and always stops above the 96px player bar.

- `surface-container-low` sheet, `radius-extra-large` on the leading corners only (proposal; current code is square).
- Header `title-large` with the count, lock and close icon buttons; tabs All / Results / Up next / Recommended as 36px pills.
- **Proposal:** section labels *Now playing* and *Up next* (`label-medium`), and the current track as a `primary-container` row whose cover becomes a `primary` tile with the animated PlayingIndicator.
- Rows 64px, 48px cover, title `body-large`, artist `body-medium`, grip handle for drag reorder (commits on pointer up), More actions.
