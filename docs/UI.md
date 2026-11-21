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
via caller-supplied colors), `M3IconButton`, `M3TextField` (`dense` 40px row
variant, label supplied by the row), `M3SearchBar`,
`M3Switch`, `M3Checkbox`, `M3Slider` (standard geometry), `M3Card`
(elevated/filled), `M3Chip`, `M3ListItem` (active/download states). All take
text via parameters and set `accessible-label` + `accessible-role`.
`Cover` (`ui/components/cover.slint`) draws artwork with a `music-2` fallback;
bar, cards, rows, detail and full player expose empty `cover` image slots until catalog wiring.

Icons: Lucide (ISC) in ui/icons, tinted via Image.colorize.

Fonts: Figtree (headings, section titles, tabs, buttons, nav labels) and Inter
(body, track lists, subtitles, lyrics) are embedded from `ui/fonts/Figtree.ttf`
and `ui/fonts/Inter-Variable.ttf`, both under SIL OFL 1.1 (`OFL-Figtree.txt`,
`OFL-Inter.txt`). `tokens.slint` imports them and exposes `Font.title` /
`Font.body`; every `Type` style carries its family, so pages never name a font.
Lyrics follow the same split: the active line is Figtree, the rest Inter.

Motion: hover/press state layers per token durations, press shape-morph
(medium 12 to small 8), card hover lift via shadow. No scale transform: Slint
has no GPU scale primitive, so press feedback is morph + state layer.

i18n: global `I18n.trl(lang, key)` (pure callback) wired from Rust to
`madrigal-i18n`, default Es. Gallery labels call `trl`; runtime `set_lang`
refreshes every binding through the `lang` property. Keys with `%1`..`%9`
use `I18n.trf(lang, key, args)`, which substitutes the arguments in Rust.
The global lives in `ui/i18n.slint` so gallery and shell share one type;
`ui/app.slint` re-exports `DemoGallery`, `MainShell` and `I18n` as the single
compile root.

Shell (`ui/shell/`, presenter `src/shell.rs`): `MainShell` window, preferred
1280×800 with a 640×480 minimum, whose sidebar and page host fill y=0 down to
the 96px Now Playing
bar, with a
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

## Adaptive layout

Breakpoints follow content, not devices; the shell and pages read
`root.width` / `root.height` and never assume a screen size.

- 1100px wide: below this the 244px sidebar becomes an 80px icon rail; above it
  stays expanded. This is where the page drops under 860px, the width Settings
  needs for its 232px group column plus a 420px field.
- 840px wide: the nowbar switches to compact and drops position, duration and
  volume; the full bar needs about 840px for its text block.
- 800px wide: the 380px queue drawer becomes a full-width sheet. The queue
  always stops above the 96px nowbar, which stays visible.
- Full player: below 1100px wide, or when the player surface is under 840px
  tall, it uses one column: tabs, a scrollable console (art, text, seek,
  transport, volume) and a pinned tool row. The two-column layout needs a 520px
  left column plus a lyrics column.
- Settings: below 860px of page width the 232px group list becomes a horizontal
  chip strip.
- Search: below 420px of page height the hint and the suggestions title hide
  and one suggestion row remains.
- Downloads: below 700px of page width the progress and speed columns hide.
- Library chip and tag rows scroll horizontally instead of wrapping.

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
progress_within}`: active-line highlight, per-word sweep when word timings
exist, click-to-seek, `markers.translation` secondary line, font/spacing from
`madrigal-settings::Lyrics`, share-as-image via `share_image::render_png`.
Position comes from `LyricsClock` driven by a 16 ms timer (frozen when paused),
and the active line is auto-scrolled to one third of the column (300 ms ease);
a manual flick pauses that for 4 s behind a tonal "Back to current line"
button. The header holds the −0.1 s / +0.1 s sync buttons and the offset value.
`QueueDrawer` tabs All/Results/Up next/Recommended from
`queue_tabs_from_queue`, drag-reorder commits on `pointer-event(up)`,
edit-lock toggle, per-row remove, current-track mark. Dialogs:
`SleepTimerDialog`, `TempoPitchDialog`, `EqualizerDialog` (10 bands +
presets), `PlaybackErrorDialog`, `StreamInfoPanel` (from
`madrigal-player::stream_info::StreamInfo`, fed by the playback snapshot and
showing an empty state when no stream is loaded). `PlaybackUiState` is injected
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

Explore moods and genres are an `M3MoodTile` grid: 4 equal columns across the
workspace minus the 24px page padding and 12px gaps (398x56 at 1920), radius 12,
1px `outline-variant`, label `Type.label-large` with a 16px left inset, hover
state layer, Tab reachable with a 2px `primary` inner focus ring. `apply_moods`
alternates entries between the left and right models, so `moods-left` fills
columns 0 and 2 and `moods-right` fills columns 1 and 3; that keeps row-major
reading order (Chill, Christmas, Commute, Energize ...) on a 4-column grid.

## Track actions

One shared `TrackMenu` (`ui/components/menu.slint`) serves every song surface:
search results, home rows and shelves, the queue, library songs and the full
player. It opens from the row "More actions" button or a right click on the row,
anchored at the pointer: 304px card, 6px inset, 48px rows with a 24px leading
icon, a 12px gap and a left-aligned label,
`surface-container-high`, elevation 2. Actions are play next, add to queue,
like or unlike, download with its live state (queued, progress, downloaded,
error), add to playlist (a second view lists the library playlists plus "New
playlist"), and go to artist or album when known. Up/Down move the selection and
skip unavailable rows, Enter activates, Escape and a click outside close, Left
returns from the playlist view. The card flips above the anchor when there is no
room below and never covers the 96px player bar; the action list scrolls when
the page area is shorter than the card. The invisible scrim covers the shell; rows use a
40px icon button with a focus ring and `action.more` as the accessible label.

## Files by task

| Area | Task | Files |
| ---- | ---- | ----- |
| Shell + foundations | F8-2 | `ui/shell/*`, `ui/gallery.slint`, `src/shell.rs`, `src/theme.rs` |
| Catalog screens | F8-3 | `ui/pages/catalog/*`, `src/pages_catalog.rs` |
| Library + downloads | F8-4 | `ui/pages/library/*`, `src/pages_library.rs` |
| Player surface | F8-5 | `ui/player/*`, `src/player_ui.rs` |
| Settings + onboarding + diagnostics | F8-6 | `ui/pages/settings/*`, `src/pages_settings.rs` |

## Page specs (1920x1080)

Common: page margin 16px from the sidebar edge and from the top of the
workspace, section gap 24px, rows 54px, content fills the workspace width, the
workspace scrolls vertically and never overlaps the 96px player bar.

Top bar (56px row: 8px top inset, 48px hit area, 44px pill; spans the
workspace, above the page): one `M3SearchBar`, 560px max width, left aligned at
the 16px page margin. Home, Search and Explore: placeholder
"search music", Enter opens Search with the query. Library and Downloads:
placeholder "search downloaded songs", filters the visible rows while typing.
`/` focuses it. Search page no longer owns a bar. The bar lives in `MainShell`
as a row above `PageHost` (`top-bar` on pages 0-2 over `search-query`,
`local-bar` on 3-4 over `local-query`, hidden on Settings).

Library, Lists tab: chips row, tag row with a labeled Tags control, one tool row
(primary New playlist button, sort tonal button, select outlined button and edit
tabs outlined button, 8px gaps), then 54px playlist rows: cover 36px, name,
"N tracks" subtitle, play icon button on the right. No cards, no blank gap.
The empty state is a list icon, "No playlists yet" and a New playlist button.

Downloads: header row (title, count over the three groups), 54px rows full
width in three groups that only appear when they have rows: "Activas"
(queued, downloading, paused), "Descargadas" (completed, also restored from the
download registry) and "Fallidas" with Retry. Rows show the 36px track cover,
title, subtitle, state or progress on the right and the matching action icon
buttons; tapping a finished row plays the local file. Empty state is a download
icon, "No downloads" and a hint pointing to the song menu > Download.

Settings: group list column 232px (16px padding, 44px items), page title at the
16px margin in `Type.title-medium`, the section pane fills the rest. Every row
is 54px: label left, one control right aligned at the 16px inset (switch, dense
field, slider, choice chips, test/reveal button). Providers are a 54px reorder
list with 24px around the block; section gaps are 24px and Diagnostics has no
second title.

Media cards (`M3Card`): 150x190, no border artifacts, no clipped shadow, hover
veil only. Rails use 12px spacing and start at the page margin.

Full player (`full.slint`): fills the workspace above the 96px bar, 16px side
margin and a 64px header (collapse chevron, title plus album line, info, more
actions and like at the end). Two columns below it: left 40% of the window (min 520px) with
the up-to-420px artwork centered, title/artist, seek row with the times at the
ends, transport (56px play), volume and the tool row; the right column takes
the rest. Tabs sit 24px under the header at the column start, the lyrics view
fills the column below them and stops at the 96px bar (idle "Sin letra", searching
bar, not found or error with Retry; the synced chip sits beside the sync control),
and the share button shares the tabs band at the end, flush with the 16px
margin. Lyrics use a 720px measure from the column start: active line
`Type.title-large-emphasized` in `primary`, other lines `Type.title-medium` in
`on-surface-variant`, both scaled by the Lyrics font setting, line gap from the
spacing setting (12px at the 1.5 default). The active line sits at y = one third
of the lyrics viewport; the sync control and the share button share the tabs
band at the end.
