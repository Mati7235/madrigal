# SearchBar

Search field at the top of Search, focused with `/`.

- **Redesign:** the pill variant of TextField, so search and every other field share one shell: 56px, `radius-full`, `surface-container-highest`, 24px leading search icon, visible placeholder, clear button once there is text.
- Focus: 2px `primary` inset ring. No separate `ring` property.
- Opens SearchSuggestions below while focused.
- Slint: `M3SearchBar` in `fields.slint` becomes `M3TextField { shape: full, leading-icon: search, clearable: true }`.
