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
`Cover` (`ui/components/cover.slint`) draws artwork with a `music-2` fallback;
bar, cards, rows, detail and full player expose empty `cover` image slots until catalog wiring.

Icons: Lucide (ISC) in ui/icons, tinted via Image.colorize.

Motion: hover/press state layers per token durations, press shape-morph
(medium 12 to small 8), card hover lift via shadow. No scale transform: Slint
has no GPU scale primitive, so press feedback is morph + state layer.

i18n: global `I18n.trl(lang, key)` (pure callback) wired from Rust to
`madrigal-i18n`, default Es. Gallery labels call `trl`; runtime `set_lang`
refreshes every binding through the `lang` property. The global lives in
`ui/i18n.slint` so gallery and shell share one type; `ui/app.slint`
re-exports `DemoGallery`, `MainShell` and `I18n` as the single compile root.

Shell (`ui/shell/`, presenter `src/shell.rs`): `MainShell` window, 1920×1080
by default, with a
`SideNav` (Home, Search, Explore, Library, Downloads, Settings; icon + label
via `trl`), a `page` int property (0..5), a `PageHost` workspace, a
NowPlayingBar placeholder strip (`player.noTrack` + `queue-count` seam), and
`ConfirmOverlay` / `PromptOverlay` under one `overlay` int enum (0 none,
1 confirm, 2 prompt; `ShellOverlay` maps it). Esc closes the open overlay,
then focuses the sidebar (`sidebar-focused`). Shortcuts arrive through the
`handle-key(string) -> bool` callback (`dispatch_key` from Rust): Space,
Left/Right (seek ∓10 via `seek-requested`), N/P, F (full player), `/` (search
focus), `1..6` / `[` / `]` (page jump/cycle). Each signal bumps an
`@testable` counter property asserted by `ui_shell`.

PageHost seam contract: `PageHost` takes `page` + `palette` and renders one
placeholder branch per page index. Screens F8-3..F8-6 replace each branch with
its page component in place, keeping the property names and the branch order;
`MainShell` only reads `page` and never reaches into page internals. The
queue count seam is `queue-count`, synced from `madrigal-player` `Queue`
length via `sync_queue_seam`; the real drawer now lives in the player surface.

Page state lives on `MainShell` as `in-out` mirror properties that `PageHost`
binds through; page callbacks record `lib-action-count` / `dl-action-count`
plus `lib-last-action` / `dl-last-action` for the checks. The presenter
(`src/pages_library.rs`) maps `madrigal-library` / `madrigal-downloads`
results into the Slint row models.

## Theme

`tokens.slint` carries two fixed expressive palettes: Light = sky blue accent
on white, Dark/PureBlack = violet accent on black. `appearance.theme` picks the
mode; matugen-derived colors for the system mode are an F9 runtime seam.

## Screen ownership (F8-3..F8-6)

| PageHost branch | Screen | Owner |
| --------------- | ------ | ----- |
| 0 Home | `HomePage`: chips, sections, quick picks, pins | F8-3 |
| 1 Search | `SearchPage`: suggestions, filters, source toggle | F8-3 |
| 2 Explore | `ExplorePage`: moods, charts, releases, browse | F8-3 |
| Detail (page-independent) | `DetailPage` behind `detail-stack`; shell callbacks `detail-requested` / `detail-opened` / `detail-closed` / `play-requested` | F8-3 |
| 3 Library | `LibraryPage` (chips, tags, sorts, multiselect, history, stats) | F8-4 |
| 4 Downloads | `DownloadsPage` (states, progress, actions) | F8-4 |
| 5 Settings | `SettingsPage` + `DiagnosticsPanel` + `OnboardingOverlay` | F8-6 |

Player surface (`ui/player/`, presenter `src/player_ui.rs`, owner F8-5):
`NowPlayingBar` replaces the shell strip (artwork thumb, title/artist,
transport, thin progress, like, queue button). `FullPlayer` overlay opens via
the shell `full-player-opened` signal (key F) and closes via its close button
or Esc (shell dispatch, `full-player-closed`); background styles
theme/gradient/blur/glow come from settings `PlayerBackground`
(`apply_background_style`), big artwork, transport + seek slider, repeat /
shuffle toggles, volume slider, lyrics + info buttons. `LyricsView` renders
`madrigal-lyrics::Lyrics` through `karaoke::{line_at, word_at,
progress_within}`: active-line highlight, per-word sweep, click-to-seek,
`markers.translation` secondary line, font/spacing from
`madrigal-settings::Lyrics`, share-as-image via `share_image::render_png`.
`QueueDrawer` tabs All/Searched/Continue/Autoplaying from
`queue_tabs_from_queue`, drag-reorder commits on `pointer-event(up)`,
edit-lock toggle, per-row remove, current-track mark. Dialogs:
`SleepTimerDialog`, `TempoPitchDialog`, `EqualizerDialog` (10 bands +
presets), `PlaybackErrorDialog`, `StreamInfoPanel` (from
`madrigal-player::stream_info::StreamInfo`). `PlaybackUiState` is injected
with `apply_playback_state`; `wire_player_callbacks` owns all player state
transitions. Asserted by `ui_player`.

Settings (F8-6): `SettingsPage` is a two-pane group list plus one section
component per group (`AppearanceSection` … `UpdatesSection`) over a generic
`SectionView` driven by the `SettingsRow` model. `settings_section` in
`src/pages_settings.rs` wires every `madrigal-settings` field explicitly
(switch/slider/text/choice/apikey/providers rows, 1:1 with the settings crate).
`OnboardingOverlay` covers first run (language, PO-provider check, optional
login, folder stub, finish sets `updates.onboarding_done`) and
`DiagnosticsPanel` shows the diag log (level filter, export, single-instance
status, latency p50/p95). `PageHost` forwards the `settings-*` seam from
`MainShell`; covered by `ui_settings`.

## Density (ui-density)

Catalog density follows the oracle: `M3ListItem` rows are 54px (title 14/500,
accent when active; subtitle/duration 11px), `M3Card` media cells are 150x190
(118px art placeholder `music-2` on `surface-container-high`, title 13/600,
subtitle 11px, hover veil 0.14) inside 190px horizontal `Flickable` rails
(spacing 12px). Text `M3Card` keeps its large padded variant for non-media use.
Search non-track results and explore charts/releases render as card rails;
track rows stay 54px lists. Sidebar is 244px wide (padding 16, item spacing 6,
item height 48).

## Files by task

| Area | Task | Files |
| ---- | ---- | ----- |
| Shell + foundations | F8-2 | `ui/shell/*`, `ui/gallery.slint`, `src/shell.rs`, `src/theme.rs` |
| Catalog screens | F8-3 | `ui/pages/catalog/*`, `src/pages_catalog.rs` |
| Library + downloads | F8-4 | `ui/pages/library/*`, `src/pages_library.rs` |
| Player surface | F8-5 | `ui/player/*`, `src/player_ui.rs` |
| Settings + onboarding + diagnostics | F8-6 | `ui/pages/settings/*`, `src/pages_settings.rs` |
