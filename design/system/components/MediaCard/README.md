# MediaCard

150×190 media cell for album, playlist and artist rails.

- `surface-container` fill, radius `radius-medium`, 118px art (`music-2` fallback on `surface-container-high`).
- Title 13/600 Figtree, subtitle 11px Inter `on-surface-variant`. Hover veil 0.14.
- **Proposal:** on hover or keyboard focus a 44px `primary` play button rises 6px into the art's bottom-right corner (150ms fade, 200ms `easing-emphasized`); it plays without opening the detail page, and its corners round from 16 to 22 under the pointer. Second card shows it.
- Elevated variant lifts `shadow-card` → `shadow-card-hover` in 200ms. Rails are 190px tall horizontal flickables, 12px gap.
- Slint: `M3Card { media, title, subtitle, cover, lit, fill, shadow-color }` in `cards.slint`; add `play-requested` beside `clicked`.
