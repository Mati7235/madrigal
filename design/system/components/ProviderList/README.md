# ProviderList

Ordered, toggleable list for the `providers` settings kind (lyrics, artwork sources).

- 54px rows: rank badge (24px `secondary-container` disc), name `body-large`, switch, move up / move down icon buttons; rows also drag-reorder.
- The rank is the fallback order; disabled providers keep their slot.
- Slint: the `providers` branch of `SectionView`.
