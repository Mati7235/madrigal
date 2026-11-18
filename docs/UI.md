# UI

Canonical M3E tokens live in `crates/madrigal-ui/ui/tokens.slint`. Ported 1:1
from `docs/DESIGN_TOKENS.md` / `docs/DESIGN_COMPONENTS.md` (seed `#ED5564`,
`tonal-spot`): 48 color roles, 30 type styles, 10 shape radii (`full` = 9999px
sentinel), 6 elevations, full duration scale, 7-step spacing.

Themes: Light, Dark (oracle tables) and PureBlack (dark roles, surface ramp at
`#000000`). `DemoGallery.palette` holds the active theme; `apply-theme(int)`
switches it at runtime. Rust `ThemeKind` maps from `madrigal-settings`
`Theme` + `pure-black` (`from_appearance`); `System` and Matugen resolve to
Dark until the system/matugen seams land.

Components in `ui/components/`: `M3Button` (filled/tonal/outlined/text/elevated
via caller-supplied colors), `M3IconButton`, `M3TextField`, `M3SearchBar`,
`M3Switch`, `M3Checkbox`, `M3Slider` (standard geometry), `M3Card`
(elevated/filled), `M3Chip`, `M3ListItem` (active/download states). All take
text via parameters and set `accessible-label` + `accessible-role`.

Motion: hover/press state layers per token durations, press shape-morph
(medium 12 to small 8), card hover lift via shadow. No scale transform: Slint
has no GPU scale primitive, so press feedback is morph + state layer.

i18n: global `I18n.trl(lang, key)` (pure callback) wired from Rust to
`madrigal-i18n`, default Es. Gallery labels call `trl`; runtime `set_lang`
refreshes every binding through the `lang` property.
