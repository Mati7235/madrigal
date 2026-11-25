# LibraryScreen

Library, Songs tab, with two rows selected. **Redesign proposal.**

- Display title with a tonal "New playlist"; primary Tabs; tag chips with the sort on the right.
- Multiselect: each row shows a checkbox; selected rows take `surface-container-highest` plus the checked box, and the SelectionBar replaces the toolbar while a selection exists.
- The playing row keeps `primary-container` and the PlayingIndicator. Selected rows never use `secondary-container`: in Light it is #e8def8 against `primary-container` #eaddff, too close to tell "selected" from "playing".
