# DetailHeader

Header of the album, playlist and artist detail page, above its track list.

- 200px art at `radius-extra-large`, a `label-medium` kind line, title `display-medium` 45/500, meta `body-medium`.
- Actions: a medium (56px) filled Play, a 56px tonal shuffle, then download, like and more as icon buttons.
- **Proposal:** the header sits on `surface-container-low` at `radius-extra-large`; with matugen the tint follows the artwork.
- Slint: `DetailPage` in `pages/catalog/detail.slint` (loading, empty and error states use `common.loading`, `detail.empty`, `detail.error`).
