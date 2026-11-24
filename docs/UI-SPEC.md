# UI spec

Visual target for the Slint UI. Source: the Madrigal design system (tokens, components, screens).
Tokens stay in `crates/madrigal-ui/ui/tokens.slint`; `design/reference/tokens.json` mirrors them plus the fixes below.

## Reference files

| Path | Content |
| --- | --- |
| `design/reference/screens/1920x1080/` | every screen, Light / Dark / Pure Black |
| `design/reference/screens/1280x720/` | every screen, Light / Dark / Pure Black |
| `design/reference/screens/1366x768/` | every screen, Dark |
| `design/reference/screens/640x480/` | every screen at the minimum size, Dark |
| `design/reference/components/<Name>.png` | one component, three themes stacked |
| `design/reference/prototype.html` | clickable prototype; open locally, uses the repo fonts |
| `design/reference/tokens.json` | tokens with the fixes applied |

Screens: `home`, `search`, `explore`, `library`, `downloads`, `settings`, `detail`, `full-player`, `queue`, plus `track-menu`, `snackbar`, `sleep-dialog` at 1920×1080 and 1280×720. File name: `<screen>-<light|dark|black>.png`.
Sample data and copy are illustrative (English); labels come from `madrigal-i18n`.

## Fixes to the current code

| Where | Now | Target | Why |
| --- | --- | --- | --- |
| `tokens.slint` Dark, PureBlack `inverse-on-surface` | `#f2eef7` | `#322f35` | equals `inverse-surface`: snackbar and tooltip text invisible (1:1); target 11.5:1 |
| `M3TextField` callers `line` | `outline-variant` | no border; filled `surface-container-highest` (see TextField) | `outline-variant` on `surface` is 1.70:1, under 3:1 |
| Playing row title | `primary` on `primary-container` | `on-primary-container` + PlayingIndicator | 4.20:1 in Light, under 4.5:1 |
| Selected row | — | `surface-container-highest` + checked box | `secondary-container` (#cfe5ff) vs `primary-container` (#d6e7ff) are indistinguishable in Light |
| `sidebar.slint` Downloads icon | `arrow-down` | `download` | every other downloads entry point uses `download` |
| `player/queue.slint` queue lock | `x` / `list-music` | `lock` / `lock-open` | neither a close nor a playlist glyph says locked |
| `player/queue.slint` row remove | `minus` | `x` | remove is a close action; `minus` reads as minimise |
| `components/menu.slint` like row | `star-filled` | `heart-filled` | star is a rating; every other like toggle uses the heart |
| `components/menu.slint` Go to album | `music-2` | `disc` | `music-2` is the missing-artwork fallback, not an album |
| `library.slint` Manage tags | `list-plus` | `tag` | adds to a list, not a tag |
| Dialogs | radius 12 | `radius-extra-large` (28) | M3 dialog shape |
| `MainShell` preferred size | 1280×800 | 1280×720 | 16:9 reference resolutions |
| Focus rings | outside the edge in places | inset 2px | parents clip outside rings |

## Build order

1. Token fixes and the `download` nav icon.
2. TextField (filled, label above, inset focus ring), TextArea, Select (tonal pill + floating menu).
3. SegmentedButton (connected group), Chip (filled, no outline), Snackbar, PlayingIndicator, Dialog shape; SearchBar becomes the TextField pill; TrackMenu takes the Select menu shell.
4. Rows and cards: ListItem playing / selected states and hover-only More button, MediaCard hover play button, DownloadRow state icons, SelectionBar.
5. Screens in PageHost order, each checked against its PNG at 1920×1080, 1366×768, 1280×720 and 640×480.
6. Full player per FullPlayer (tint from the artwork, transport sizes, karaoke sweep, compact under 600px tall), then the wavy seek in the bar.
7. Motion: MotionCurrent stays; remaining MotionProposed items one at a time.

## Done when

- The running app PNG matches the reference PNG for the same screen, size and theme: layout, sizes, radii, colours and type styles; sample text may differ.
- Every text pair meets 4.5:1 (3:1 at 24px+ and for control boundaries) in Light, Dark and Pure Black.
- Nothing scales; motion is shape, colour, opacity and offset only. Reduced motion stops loops.

## Foundations

Madrigal is a Rust desktop music player (local library and YouTube Music, synced lyrics, scrobbling, MPRIS) whose interface is built in Slint on **Material 3 Expressive**. This system is the web mirror of `crates/madrigal-ui/ui/tokens.slint` and `ui/components/`; the Slint files stay the source of truth, and `docs/UI.md` holds the direction.

### Principles

- **Music first, chrome second.** Surfaces are quiet tonal steps (`surface` → `surface-container-highest`); the only loud colour is `primary`, kept for the thing that is playing or the action that plays it.
- **Expressive through shape and motion, not decoration.** Corners morph on press (`radius-medium` 12 → `radius-small` 8 in `duration-pressed` 150ms), selected icon buttons go from a disc to a rounded square, switch thumbs grow 16 → 24px. No gradients, no scale transforms.
- **Dense where it scans, roomy where it shows.** Track rows are 54px; media cards are 150×190 with 118px art; sections breathe at `space-24`.
- **Keyboard is a first-class pointer.** Every control is Tab reachable and draws a 2px focus ring in its content colour, inset 2px.
- **Two type voices.** Figtree speaks (titles, buttons, nav, tabs, the active lyric line); Inter reads (track lists, subtitles, the rest of the lyrics).

### Themes

| Theme | Ground | Accent | Notes |
| --- | --- | --- | --- |
| Light | `surface` #ffffff | `primary` #0b6bcb sky blue | containers are cool blue tints (#f4f8fe → #d8e3f2) |
| Dark | `surface` #0b0810 | `primary` #cfbcff violet | containers are violet-black (#120e19 → #292234) |
| Pure Black | `surface` #000000 | `primary` #cfbcff violet | Dark roles with the surface ramp shifted to #000000 for OLED |

`appearance.theme` picks the mode; `pure-black` swaps the Dark ramp. A matugen-derived palette (from the wallpaper) for the System mode is a planned runtime seam that fills the same 48 roles, so components must only ever reference roles, never hex.

### Colour rules

- Text sits on its pair: `on-surface` / `on-surface-variant` on any `surface*`, `on-primary` on `primary`, `on-primary-container` on `primary-container`, and so on. Never put `primary` text on `primary-container`.
- **Filled** = `primary` + `on-primary` (one per view: Play, Save, Continue). **Tonal** = `secondary-container` + `on-secondary-container`. **Outlined** = transparent, `outline-variant` border, `on-surface-variant` label. **Text** = transparent, `on-surface-variant`. **Elevated** = `surface-container-low` + `primary` label.
- The playing row uses `primary-container` as its fill with `on-primary-container` text and the PlayingIndicator; never `primary` text on `primary-container` (4.20:1 in Light). Hovered rows use `surface-container`.
- `primary` text on `surface-container-high` is 4.42:1 in Light: keep it to 18px+ or bold there, or use `on-surface`.
- Selected chips use `secondary-container`; unselected chips are `outline`-bordered with `on-surface-variant` text. Segmented buttons are a connected group with no outline: `surface-container-highest` segments, the selected one a `primary` pill with a check.
- Focus rings are drawn inside the control's edge (inset), never outside, so no container clips them. Only one menu is open at a time.
- Dividers and tile borders are 1px `outline-variant`; control borders that remain (outlined button, chip, switch off) are `outline`. Never `outline-variant` on a control: 1.70:1 on `surface` in Light, under the 3:1 a control boundary needs.
- Text fields are filled and borderless: `surface-container-highest`, `radius-large` (pill for search and dense rows), label above, placeholder visible, 2px `primary` focus ring, errors with an icon. Dropdowns are tonal pills that open a floating `radius-large-increased` menu with two-line options and a check on the chosen one. See TextField, TextFieldStates, TextArea, Select.
- Errors (failed download, playback error) use `error` / `error-container`; never for likes or emphasis.
- Snackbars and plain tooltips use `inverse-surface` with `inverse-on-surface` text and `inverse-primary` actions: a light chip on dark themes, a dark chip on Light. **Fix:** `tokens.slint` gives Dark and PureBlack the same value (#f2eef7) for both, which makes the text invisible (1:1); this system uses #322f35 (11.5:1).

### State layers

Every interactive surface draws a layer of its **content colour** over its fill: `state-hover` 0.08, `state-pressed` 0.1, media card hover `state-media-hover` 0.14. Disabled: fill = `on-surface` at `state-disabled-container` 0.12, content = `on-surface` at `state-disabled-content` 0.38. Layers fade in `duration-hover` 100ms ease-in-out.

### Type

30 styles: the M3 scale (display, headline, title, body, label × large/medium/small) plus an *emphasized* twin of each (weight 500 or 700). Display, headline, title and label large/medium are Figtree; body and `label-small` are Inter. Pages never name a font: they pick a style.

- Row title: `title-small` 14/500; row subtitle and duration: `label-small` 11px.
- Media card title `title-small`-weight 600 at 13px, subtitle 11px (density spec in `docs/UI.md`).
- Buttons, chips, tabs, menu rows: `label-large` 14/500.
- Line heights follow the M3 baseline scale (Slint tokens carry size, weight and tracking only).

### Shape

`radius-extra-small` 4 · `small` 8 · `medium` 12 · `large` 16 · `large-increased` 20 · `extra-large` 28 · `extra-large-increased` 32 · `extra-extra-large` 48 · `full` (pill).

Small buttons rest at 12 and press to 8; medium buttons rest at 16 and press to 12. Rows and menu rows are 8. Media cards, mood tiles and the track menu are 12. Chips, switches, search bar, segmented buttons and icon buttons are pills.

### Spacing and layout

Seven steps: 4 · 8 · 12 · 16 · 24 · 32 · 48. Page margin 16px from the sidebar and the workspace top; section gap 24; card rails gap 12.

Reference resolutions are 16:9: 1920×1080 first, then 1366×768 and 1280×720, plus a 640×480 tile for the minimum. Every screen must fit them with the player bar fully visible and no horizontal overflow.

Shell: 244px sidebar (48px items, 16 padding, 6 gap) that collapses to an 80px rail under 1100px; 96px Now Playing bar always visible; 380px queue drawer that becomes a full-width sheet under 800px. Breakpoints follow content, not devices (full table in `docs/UI.md`).

### Elevation and motion

Elevation is used sparingly: elevated cards (`shadow-card`, lifting to `shadow-card-hover` on hover in `duration-fade` 200ms ease-out) and the track menu (`shadow-menu`). Everything else is separated by tonal surface steps.

Motion: hover 100ms, press morph 150ms, fades 200ms, lyric auto-scroll 300ms ease. The full M3 duration scale (50ms → 1000ms) is in tokens. Expressive feedback is shape and colour, never scale (Slint has no scale primitive): buttons morph 12 → 8 on press, the play button is a disc when paused and a `radius-small` square while playing, switch thumbs grow 16 → 24px. The **MotionCurrent** card loops each of these.

### Intentional additions

**MotionProposed** specifies M3 Expressive motion not yet in the app, each within Slint's limits: a wavy seek bar while playing, a connected transport group where the pressed button widens, a nav pill that stretches between items, a shape-morphing loader for buffering and downloads, and a lyric line that eases colour and weight in. Approximate spring overshoot with `ease-out-back` or a `cubic-bezier`; every animation stops under reduced motion.

### Redesign proposals

Where this system goes past the current code, the card says **Proposal** or **Intentional addition**. The direction, in one line each:

- **Hierarchy by size and shape, not colour.** One big type moment per page (`display-medium-emphasized` page titles), one oversized control per surface (72px play in the full player), shape variety confined to hero rows.
- **Motion with meaning.** M3 emphasized easing for anything that moves through space (`easing-emphasized-decelerate` in, `easing-emphasized-accelerate` out), standard easing for colour and opacity; expressive spatial springs where Slint can approximate them.
- **Playing state is a glyph.** The PlayingIndicator marks the current track everywhere: rows, queue, cards. Colour alone never says "playing".
- **Feedback for silent actions.** Snackbar after queue, like and download actions; EmptyState for empty pages.
- **Softer, larger dialogs.** `radius-extra-large`, centred icon and title.
- **Screens:** every page composed at 1280×720 (HomeScreen, SearchScreen, ExploreScreen, LibraryScreen, DownloadsScreen, SettingsScreen, DetailScreen, FullPlayer, QueueDrawer) plus CompactShell at 800×450 for the breakpoints.
- **Selected is not playing.** Selected rows use `surface-container-highest` and a checked box; `secondary-container` is too close to `primary-container` in Light (#cfe5ff vs #d6e7ff) to mark selection.

### Iconography

Lucide (ISC), 24px grid, 2px stroke, round caps and joins. Drawn at 20px in buttons and 24px in menu rows, tinted with the content colour of the control (`Image.colorize` in Slint; inline SVG with `currentColor` on the web). Filled variants (`heart-filled`, `star-filled`) mean the toggled-on state. Missing artwork shows `music-2` on `surface-container-high`.

Icon map. One glyph per meaning; the name is the sprite id in `prototype.html` and the SVG name in `ui/icons/`.

| Meaning | Icon |
| --- | --- |
| Home | `house` |
| Search | `search` |
| Explore, discover, internet | `compass` |
| Library | `library` |
| Downloads, download action | `download` |
| Settings | `settings` |
| Play / pause | `play` / `pause` |
| Previous / next | `skip-back` / `skip-forward` |
| Shuffle | `shuffle` |
| Repeat all / repeat one | `repeat` / `repeat-1` |
| Like / liked | `heart` / `heart-filled` |
| Share | `share-2` |
| Queue, add to queue | `list-music` |
| Play next, add to list | `list-plus` |
| Album | `disc` |
| Artist, account | `user-round` |
| Storage | `hard-drive` |
| Privacy | `shield` |
| Tags | `tag` |
| Updates, retry | `rotate-cw` |
| Sleep timer | `clock` |
| Speed, tempo | `gauge` |
| Equalizer, appearance | `sliders-horizontal` |
| Lyrics | `mic` |
| Details | `info` |
| Error | `triangle-alert` |
| Done | `circle-check` |
| Close, cancel, remove | `x` |
| More actions | `ellipsis-vertical` |
| Sort | `arrow-down-up` |
| Missing artwork | `music-2` |

`share-2`, `hard-drive`, `disc`, `shield`, `tag` and `arrow-down-up` are not in `ui/icons/` yet; the reference uses them and the port adds them with the icon pass.

### Voice

Short, sentence case, verb first: "Add to queue", "Play next", "Back to current line", "New playlist". No exclamation marks. Labels come from `madrigal-i18n` (default Spanish); English strings here are reference.

### Not mirrored

- Components are static HTML/CSS renditions hand-written from the Slint sources in `ui/components/`; the Slint library itself cannot run on the web.
- Playfair Display is embedded by `app.slint` but no style uses it; it is left out.
- The FigCraft mockups in `design/` (paper #F4F1EA, wine accent) are a superseded direction and are not part of this system.
- Shadow values are read from the components' `drop-shadow-*` properties; the `shadow` role is #000000 in every theme.

## Actions

### Button

Reference: `design/reference/components/Button.png`.

Common button in five emphasis levels; the caller supplies the colours from palette roles.

- **Filled** `primary`/`on-primary`: the one main action of a view (Play, Continue).
- **Tonal** `secondary-container`/`on-secondary-container`: strong secondary (Shuffle, Back to current line).
- **Outlined** `outline-variant` border, `on-surface-variant` label; **Text** label only; **Elevated** `surface-container-low` + `primary`.
- Sizes: `small` 40px (radius 12 → 8 pressed, 16px padding), `medium` 56px (16 → 12, 24px padding). Label `label-large`, elided.
- Disabled: `on-surface` at 0.12 fill, 0.38 label. Focus: 2px ring in the content colour, inset 2px.
- Slint: `M3Button { text, size, enabled, fill, content, line, state-on-surface, clicked }` in `buttons.slint`.

### IconButton

Reference: `design/reference/components/IconButton.png`.

40px icon-only button; round at rest, morphs to a `radius-small` square when `selected`.

- Icon 20px, tinted with `content` (or `icon-color`). Hover layer 0.08 of content.
- Transport (previous, play/pause, next), toggles (shuffle, repeat, like), row "More actions".
- Toggled-on state: `selected: true` plus a tonal fill, or the filled icon variant (`heart-filled`).
- Always pass an accessible label (`action.more`, `player.next`...).
- Slint: `M3IconButton { icon, selected, fill, content, line, clicked }`.

### SelectionBar

Reference: `design/reference/components/SelectionBar.png`.

Contextual toolbar while rows are multi-selected in the library.

- **Proposal:** a floating 64px `secondary-container` bar at `radius-extra-large` with `shadow-card`, replacing the page header while a selection exists, so the count and the bulk actions sit in one place.
- Leading clear button, count `title-medium` (`library.selected`), trailing icon actions: play next, add to playlist, download, remove.
- Esc clears the selection.

## Selection

### Checkbox

Reference: `design/reference/components/Checkbox.png`.

18px box in a 40px target for library multiselect.

- Checked: `primary` fill with `on-primary` check. Unchecked: 2px `on-surface-variant` border, radius 5.
- Slint: `M3Checkbox { checked, selected-fill, selected-content, unselected-line }`.

### Chip

Reference: `design/reference/components/Chip.png`.

32px filter chip for search filters, library tags and dialog options.

- **Redesign proposal:** no outline. Unselected chips are filled `surface-container-highest` with `on-surface` labels; the selected chip turns `secondary` / `on-secondary`, gains a 14px check and tightens its corners from 16 to 10 (200ms `easing-emphasized`).
- Selection reads by fill, check and shape. `secondary` keeps chips apart from the `primary` SegmentedButton pill and from `primary-container` playing rows.
- Label `label-large`; hover 0.08 layer; keyboard focus 2px inset ring. Rows of chips scroll horizontally, never wrap.
- Contrast: `on-secondary` on `secondary` 6.5:1 (Light) / 11.9:1 (Dark); `on-surface` on `surface-container-highest` 14:1 / 13:1.
- Slint: `M3Chip { text, selected, selected-fill, selected-content, unselected-content, unselected-line }` in `cards.slint`; `unselected-line` goes, `unselected-fill` arrives.

### SegmentedButton

Reference: `design/reference/components/SegmentedButton.png`.

40px pill group for mutually exclusive views (queue tabs, theme mode).

- **Redesign proposal:** an M3 Expressive connected button group. No outline: each segment is its own `surface-container-highest` button, 2px apart; inner corners 8px, outer corners pill.
- The selected segment becomes a full pill in `primary` / `on-primary` with a check, morphing its corners in 200ms `easing-emphasized`. Selection reads by fill, shape and check, never by colour alone.
- Labels `label-large`; hover adds the 0.08 state layer; keyboard focus draws a 2px inset ring.
- Used for the queue tabs, theme mode, equalizer presets and the diagnostics level filter.
- Slint: `M3SegmentedButton` in `selection.slint`.

### Slider

Reference: `design/reference/components/Slider.png`.

M3 Expressive slider: 16px track split by a 4×44 handle with a 6px gap.

- Active track and handle `primary`; inactive track `secondary-container`.
- Seek bar in the full player, volume, equalizer bands, tempo and pitch.
- Slint: `M3Slider { value, active-track, inactive-track, handle-color }`.

### SourceToggle

Reference: `design/reference/components/SourceToggle.png`.

Online / Local switch for search (`catalog.online`, `source.local`).

- **Proposal:** a 40px pill track in `surface-container-high` with a raised `surface` thumb (`shadow-card`) under the active side, icons + labels; different from SegmentedButton because it switches the data source, not a filter.

### Switch

Reference: `design/reference/components/Switch.png`.

52×32 toggle for settings rows.

- On: `primary` track, 24px `on-primary` thumb. Off: `surface-container-highest` track, 2px `outline` border, 16px `outline` thumb.
- Transition 200ms ease-in-out. Disabled: `on-surface` 0.12 track, 0.38 thumb.
- Slint: `M3Switch { checked, on-track, on-thumb, off-track, off-line, off-thumb }`.

## Inputs

### ProviderList

Reference: `design/reference/components/ProviderList.png`.

Ordered, toggleable list for the `providers` settings kind (lyrics, artwork sources).

- 54px rows: rank badge (24px `secondary-container` disc), name `body-large`, switch, move up / move down icon buttons; rows also drag-reorder.
- The rank is the fallback order; disabled providers keep their slot.
- Slint: the `providers` branch of `SectionView`.

### SearchBar

Reference: `design/reference/components/SearchBar.png`.

Search field at the top of Search, focused with `/`.

- **Redesign:** the pill variant of TextField, so search and every other field share one shell: 56px, `radius-full`, `surface-container-highest`, 24px leading search icon, visible placeholder, clear button once there is text.
- Focus: 2px `primary` inset ring. No separate `ring` property.
- Opens SearchSuggestions below while focused.
- Slint: `M3SearchBar` in `fields.slint` becomes `M3TextField { shape: full, leading-icon: search, clearable: true }`.

### SearchSuggestions

Reference: `design/reference/components/SearchSuggestions.png`.

Search history and suggestions under the focused SearchBar.

- `surface-container-high` sheet at `radius-extra-large`; the field inside uses `surface-container-highest`.
- Groups: Recent (history icon, per-item remove, Clear all) and Suggestions (search icon, arrow to fill the field).
- **Proposal:** the typed part shows in plain weight and the completion in semibold `on-surface`, so the eye reads what is new.
- Below 420px of page height the hint and the suggestions title hide and one suggestion row remains.

### Select

Reference: `design/reference/components/Select.png`.

Dropdown: a tonal pill that opens a floating menu. **Redesign proposal**, replacing the field-shaped picker.

- Trigger: 48px pill (`radius-full`) in `surface-container-highest` with the value in `title-medium` and a chevron that turns 180° when open; while open it switches to `secondary-container`. Dense 40px pill in settings rows. The label, when needed, sits above like TextField.
- Menu: detached 8px below the trigger, `surface-container-high`, `radius-large-increased` (20), 8px inner padding, `shadow-menu`; enters with a 4px drop and fade in 200ms `easing-emphasized-decelerate`. Aligns to the trigger's start edge (end edge when near the window edge).
- Options: 48px minimum, `radius-medium` (12) inside the menu, a 24px leading slot, title `body-large`, optional description `body-small` in `on-surface-variant`. The chosen option shows a `primary` check in the leading slot; colour never marks it alone.
- Hover adds the 0.08 layer; keyboard focus draws a 2px `on-surface` ring around the option (shown on "Automatic").
- Unavailable options stay listed at 0.38 with a reason in the description. A 1px `outline-variant` divider separates groups; a trailing chevron opens a submenu.
- Only one menu is ever open: opening one closes any other; a click outside or Esc closes it and returns focus to its trigger.
- Keyboard: Enter / Space / Down open, Up / Down move, Enter picks, Esc closes; `aria-haspopup="listbox"`, `aria-expanded`, `aria-selected`.

### SettingsRow

Reference: `design/reference/components/SettingsRow.png`.

One settings row, driven by the `SettingsRow` model; its `kind` picks the trailing control.

- Kinds: `switch` (M3Switch), `choice` with up to 4 options (SegmentedButton) or more (Select pill, menu opens toward the row's end edge), `slider` (value label on the right), `text` (dense 40px pill TextField, the row title is its label), `apikey` (masked dense pill with Show / Hide).
- Title `body-large`, supporting line `body-medium` `on-surface-variant`; rows at least 64px.
- **Proposal:** group rows into `surface-container-low` cards at `radius-large` with a `primary` group label and 1px `outline-variant` dividers; advanced rows stay hidden behind "Show advanced".
- Slint: `SectionView` in `pages/settings/section.slint`, wired 1:1 to `madrigal-settings` by `settings_section`.

### TextArea

Reference: `design/reference/components/TextArea.png`.

Multi-line text for playlist descriptions and feedback.

- Same shell as TextField: label above, `surface-container-highest`, `radius-large`, 2px `primary` focus ring.
- Starts at 120px and grows with the content to 180px, then scrolls. Counter always visible; past the limit the field turns to the error state.

### TextField

Reference: `design/reference/components/TextField.png`.

Text input: a filled, borderless container with its label above. **Redesign proposal**, replacing the outlined field; try typing in the preview.

- Label sits **above** the field in `label-large` (`on-surface-variant`, `primary` while focused). It never moves, shrinks or hides, so there is no notch and no jump.
- Container 56px, `surface-container-highest`, `radius-large` (16). Pill variant (`radius-full`, 28) for search and one-line queries. No border at rest; hover adds the 0.08 state layer.
- Focus: a 2px `primary` ring drawn inside the container edge, so no parent can clip it. Error: a 2px `error` ring, an `error` label, a trailing warning icon and supporting text that starts with the icon.
- Value `body-large` `on-surface`; placeholder `on-surface-variant` stays visible until you type. Supporting text and counter `body-small` 6px below.
- Accessories: 24px leading icon, a unit suffix (`s`, `kbps`, `%`), a clear button once there is a value, Show / Hide for secrets.
- Contrast: with no border, the field is told from the page by its fill plus its visible label and placeholder (`on-surface-variant` is 7.17:1 on the container in Light). Do not drop both the label and the placeholder.
- Slint: `M3TextField` in `components/fields.slint`. Today it is outlined with an in-box floating label; the port drops `line` / `focus-line` for `fill` (`surface-container-highest`) and a `ring` colour, adds `shape` (large / full), `leading-icon`, `suffix`, `max-length`, `clearable`, `secret`.

### TextFieldAnatomy

Reference: `design/reference/components/TextFieldAnatomy.png`.

Parts of a text field and their tokens.

- Label above in `label-large`, 4px inset, 6px to the container.
- Container: 56px (40px dense pill), 20px start padding (16px with a leading icon), 8px end padding around a 40px trailing action, `radius-large` or `radius-full`.
- Leading icon 24px with a 12px gap; value `body-large`; supporting text and counter `body-small`, 6px below.

### TextFieldStates

Reference: `design/reference/components/TextFieldStates.png`.

Every state of the default field and the dense pill, side by side.

- Enabled: `surface-container-highest`, no border. Hover: +0.08 `on-surface` layer.
- Focused: 2px `primary` ring, label in `primary`. Filled: value in `on-surface`.
- Error: 2px `error` ring, label, icon and supporting text in `error`; `aria-invalid` set.
- Disabled: container `on-surface` 0.06, all text 0.38.
- Dense pill (40px, radius 20, `body-medium` value) is for settings rows, where the row title is the label.

## Navigation

### NowPlayingBar

Reference: `design/reference/components/NowPlayingBar.png`.

The persistent player strip: a floating card in the 96px band at the bottom of the shell, always visible.

- Card inset 8px, radius 24px, `surface-container-high`, 0.5px `outline-variant` hairline.
- 48px cover, title Inter 13/700 `on-surface`, artist 11px `on-surface-variant`.
- Transport: previous, play/pause as a filled `primary` icon button that morphs from a disc (paused) to a `radius-small` square (playing), next.
- Seek: 4px `surface-container-highest` track and a 4×20 `primary` handle; the played part waves while playing (the same fixed-wavelength, clipped wave as the full player) and is a flat `primary` line when paused. Volume: 4px track, `primary` fill.
- Like and queue are 34×40 mini buttons (radius 17 → 8 when selected, liked = `primary` fill).
- Below 840px it goes `compact`: position, duration and volume drop. The queue drawer stops above it.
- Slint: `NowPlayingBar` in `player/bar.slint`.

### SideNav

Reference: `design/reference/components/SideNav.png`.

Main navigation: a 244px sidebar that collapses to an 80px icon rail below 1100px of window width.

- `surface-container-low` panel, 16px padding, 6px between items.
- Items are 48px pills: 22px icon, 12px gap, `label-large` label. Active: `secondary-container` / `on-secondary-container`; rest: `on-surface-variant`.
- Rail: icons centred, labels move to a tooltip (`tip-text`). Focus: 2px `primary` ring inset 2px.
- Pages: Home, Search, Explore, Library, Downloads, Settings; keys `1`..`6`, `[` / `]` cycle.
- Icons: `house`, `search`, `compass`, `library`, `download`, `settings`. **Fix:** `sidebar.slint` uses `arrow-down` for Downloads; use `download` so the nav matches every other downloads entry point (Home shortcut, track menu, detail header).
- Slint: `SideNav { palette, page, expanded }` in `shell/sidebar.slint`.

### Tabs

Reference: `design/reference/components/Tabs.png`.

Library's primary tabs and the tag chip row beneath them.

- **Proposal:** M3 primary tabs: 48px, `label-large`, active tab `primary` text with a 3px rounded `primary` indicator inset 12px, 1px `outline-variant` baseline. Today the library tabs are pills; pills stay for secondary switches (queue tabs, player tabs).
- Tabs: Library, Songs, Albums, Artists, Playlists, History, Stats.
- Tag row: filter chips with an "All" chip first, a text-style "New tag" chip, and Edit / Done on the right; the row scrolls horizontally, never wraps.
- Slint: `LibraryPage` in `pages/library/library.slint`.

## Content

### ArtistCard

Reference: `design/reference/components/ArtistCard.png`.

MediaCard variant for artists.

- **Proposal:** circular 118px art (`radius-full`) with centred name and kind, so artists read differently from albums in mixed rails and search results.
- Same 150×190 cell, hover veil and focus ring as MediaCard.

### DetailHeader

Reference: `design/reference/components/DetailHeader.png`.

Header of the album, playlist and artist detail page, above its track list.

- 200px art at `radius-extra-large`, a `label-medium` kind line, title `display-medium` 45/500, meta `body-medium`.
- Actions: a medium (56px) filled Play, a 56px tonal shuffle, then download, like and more as icon buttons.
- **Proposal:** the header sits on `surface-container-low` at `radius-extra-large`; with matugen the tint follows the artwork.
- Slint: `DetailPage` in `pages/catalog/detail.slint` (loading, empty and error states use `common.loading`, `detail.empty`, `detail.error`).

### DiagnosticsPanel

Reference: `design/reference/components/DiagnosticsPanel.png`.

Diagnostics log in Settings: level filter, latency, export.

- Level filter as a segmented button (All / Info / Warn / Error), latency p50 / p95 as `surface-container-highest` tags, Export as an outlined button.
- Log on `surface-container-lowest` with a hairline, monospace 12/20; levels coloured `secondary` / `tertiary` / `error` and always spelled out, never colour alone.
- Slint: `DiagnosticsPanel` in `pages/settings/diagnostics.slint`. The lines here are illustrative.

### DownloadRow

Reference: `design/reference/components/DownloadRow.png`.

A 54px download row with every state the downloads crate reports.

- Columns: 36px cover, title / kind, state (110px), 120×4 progress, speed (72px, only while downloading), actions (92px).
- States: Queued (indeterminate bar), Downloading (determinate + speed, pause and cancel), Paused (`outline` fill, resume), Done (`primary`, check icon), Error and Cancelled (`error`, warning icon, retry and remove).
- **Proposal:** a state icon beside the word, so done and error differ by shape as well as colour; the indeterminate bar uses `easing-emphasized`.
- Below 700px of page width the progress and speed columns hide.
- Slint: `DownloadsPage` in `pages/library/downloads.slint`.

### EmptyState

Reference: `design/reference/components/EmptyState.png`.

Empty pages (downloads, library, history, search with no results). **Intentional addition** for a consistent tone.

- A 120px organic blob (mixed radii 40–60px) in `secondary-container` holding a 48px icon, `headline-small` title, one `body-medium` sentence, one tonal action.
- One per page, centred in the workspace. No illustrations or emoji.

### ListItem

Reference: `design/reference/components/ListItem.png`.

54px track row used by every song list: search, library, queue, downloads.

- 36px cover, title `title-small` 14/500, subtitle and duration `label-small` 11px `on-surface-variant`.
- Hover `surface-container`; playing row `primary-container` with `on-primary-container` text and the animated PlayingIndicator (a `primary` title there is 4.20:1 in Light, under the floor).
- **Proposal:** the trailing "More actions" button shows only on hover, keyboard focus and the playing row (second row shows hover), so long lists read as text, not a column of icons. It stays in the tab order and keeps its accessible label; right click opens the same TrackMenu.
- Download state: 120×6 progress bar (`surface-variant` track, `primary` fill).
- Slint: `M3ListItem { title, subtitle, duration, active, progress, menu, row-bg, active-bg, accent }` in `rows.slint`.

### LyricsView

Reference: `design/reference/components/LyricsView.png`.

Synced lyrics: the active line follows playback and can be tapped to seek.

- Header: a `Synced` chip (`secondary-container`) or `surface-container-highest` when unsynced, and the −0.1 s / +0.1 s offset buttons around the current offset.
- Lines Inter `title-medium`-size and up in `on-surface-variant`; the active line Figtree, with a per-word sweep in `primary` when word timings exist; translation (`markers.translation`) under it.
- Auto-scroll keeps the active line at one third of the column (300ms ease). A manual flick pauses it for 4s behind the tonal **Back to current line** button.
- **Proposal:** past lines at 0.6 opacity and a larger active line (28/700) so the eye lands on it from across the room.

### MediaCard

Reference: `design/reference/components/MediaCard.png`.

150×190 media cell for album, playlist and artist rails.

- `surface-container` fill, radius `radius-medium`, 118px art (`music-2` fallback on `surface-container-high`).
- Title 13/600 Figtree, subtitle 11px Inter `on-surface-variant`. Hover veil 0.14.
- **Proposal:** on hover or keyboard focus a 44px `primary` play button rises 6px into the art's bottom-right corner (150ms fade, 200ms `easing-emphasized`); it plays without opening the detail page, and its corners round from 16 to 22 under the pointer. Second card shows it.
- Elevated variant lifts `shadow-card` → `shadow-card-hover` in 200ms. Rails are 190px tall horizontal flickables, 12px gap.
- Slint: `M3Card { media, title, subtitle, cover, lit, fill, shadow-color }` in `cards.slint`; add `play-requested` beside `clicked`.

### MoodTile

Reference: `design/reference/components/MoodTile.png`.

56px tile of the Explore moods and genres grid (4 columns, 12px gaps).

- 1px `outline-variant` border, radius `radius-medium`, `label-large` with a 16px inset, hover layer.
- Focus: 2px `primary` inner ring.

### PlayingIndicator

Reference: `design/reference/components/PlayingIndicator.png`.

Three pill bars that bounce while a track plays. **Proposal**, and the fix for a contrast failure.

- Today the playing row sets its title in `primary` on `primary-container`: 4.20:1 in Light, under the 4.5:1 floor. The fix: title and metadata in `on-primary-container` (13.7:1), and "this is playing" carried by this indicator instead of by title colour.
- 14×14, three 4px pills, 2px gap, 900ms ease-in-out, staggered by 300ms; `primary` on surfaces, `on-primary-container` inside the playing row.
- Paused: the bars freeze at their rest heights. Reduced motion: frozen always.

### SectionHeader

Reference: `design/reference/components/SectionHeader.png`.

Title row above every rail and list section.

- `title-large` 22/500 Figtree, then an optional "See all" text button and, for horizontal rails, scroll-left / scroll-right icon buttons.
- 12px below the header, 24px between sections.

### StatsCard

Reference: `design/reference/components/StatsCard.png`.

Listening stats for the Library "Stats" tab (`library.stats.year`, `library.stats.top`).

- **Proposal:** three stat tiles with one big number each (`display-medium` 45px, tabular figures), the headline tile in `primary-container`; then a top-artists list with pill bars (`primary` for the first, `secondary` for the rest) and hours on the right.
- Numbers come from the scrobble history; the example values here are placeholders.
- One highlighted tile per view.

### TrackMenu

Reference: `design/reference/components/TrackMenu.png`.

The one context menu for every song surface.

- **Redesign:** same shell as the Select menu: 304px, `surface-container-high`, `radius-large-increased` (20), 8px inner padding, `shadow-menu`.
- Header: 40px cover, title `title-medium`, artist · album `body-small`; a 1px `outline-variant` divider.
- Rows: 48px, `radius-medium` (12) inside the card, 24px `on-surface-variant` icon, 12px gap, label `body-large`. Groups: playback and library actions, then navigation, split by a divider.
- States: hover 0.08 layer; keyboard focus a 2px inset `on-surface` ring (first row); a toggled action shows its filled icon in `primary` ("Liked"); unavailable rows at 0.38 (Go to artist when unknown); "Add to playlist" carries a chevron and opens its second view.
- Keyboard: Up/Down skip unavailable rows, Enter activates, Esc closes, Left leaves the playlist view.
- Placement: anchored at the pointer or the row's More button, flips above when there is no room, never covers the 96px player bar.
- Slint: `TrackMenu` in `menu.slint`.

## Feedback

### Progress

Reference: `design/reference/components/Progress.png`.

Progress indicators and badges.

- Linear: 4px, `primary` on `secondary-container`; indeterminate slides with `easing-emphasized`. Download rows keep their 4px bar on `surface-container-highest`.
- Circular: 48px, 4px stroke, round caps; determinate for known work, spinning for `common.loading`.
- Badges: a 6px `error` dot for "something new", a numeric `error` badge for counts, and text labels for track facts (`tertiary-container` for quality, `surface-container-highest` for LRC or explicit).
- **Proposal:** these replace ad-hoc loading text; the wavy seek bar (MotionProposed) is the expressive sibling for playback.

## Overlays

### ConfirmDialog

Reference: `design/reference/components/ConfirmDialog.png`.

The shell's two generic overlays: `ConfirmOverlay` and `PromptOverlay` (`overlay` 1 and 2), over a `scrim`.

- Confirm: icon, title, one sentence, Cancel + confirm. A destructive confirm is a filled `error` button; everything else stays `primary`.
- Prompt: title, one TextField (focused on open, label above), Cancel + OK (`prompt.ok`). Enter submits, Esc closes.
- **Proposal:** dialog shape `radius-extra-large` (today 12px) and the scrim at 0.32.

### Dialog

Reference: `design/reference/components/Dialog.png`.

Modal dialogs: sleep timer, tempo and pitch, equalizer, playback error, confirm and prompt.

- **Proposal:** M3 dialog shape `radius-extra-large` (28) instead of the current 12, a 24px hero icon (`secondary`, or `error` for failures), centred `headline-small` title, `body-medium` supporting text.
- `surface-container-high`, `shadow-menu`, over a `scrim`. Actions trail right: text button to dismiss, one filled or tonal confirm.
- Options as chips (sleep: 15 / 30 / 45 / 60 min, End of track). Esc closes; focus stays inside.

### Equalizer

Reference: `design/reference/components/Equalizer.png`.

Ten-band equalizer with presets.

- Presets Flat / Bass / Treble / Vocal (`audio.preset.*`) as a segmented button; on/off switch in the header.
- **Proposal:** bands as 16px expressive vertical sliders (`secondary-container` track, `primary` fill, pill ends) labelled 31 Hz → 16 kHz in `label-small`, replacing plain vertical sliders.
- Dialog shape and actions as in Dialog.

### Onboarding

Reference: `design/reference/components/Onboarding.png`.

First-run flow: language, PO-provider check, optional login, music folder, finish (sets `updates.onboarding_done`).

- `surface-container-high` card at `radius-extra-large`, a segmented progress bar where the current step is twice as wide, step label `label-medium`, title `headline-medium`.
- Choices as 56px `radius-large` tiles; selected = `secondary-container` with a check.
- Skip (text) and Next / Finish (filled); Esc never dismisses the last step.
- Slint: `OnboardingOverlay` in `pages/settings/onboarding.slint`.

### Snackbar

Reference: `design/reference/components/Snackbar.png`.

Short confirmation after an action, with at most one action. **Intentional addition**: Madrigal has no toast today, and track actions (play next, add to queue, like, download) finish silently.

- **Expressive shape:** one-line snackbars are full pills (56px, `radius-full`); two-line ones use `radius-large-increased` (20) with the action under the text. No square corners.
- `inverse-surface` / `inverse-on-surface`, a 24px leading icon in `inverse-primary` that says what happened, the action as a filled `inverse-primary` pill, an optional dismiss button. `shadow-menu`.
- Enters by rising 16px with a fade in 300ms `easing-emphasized-decelerate`; leaves with `easing-emphasized-accelerate`. Nothing scales.
- Sits 16px above the player bar, centred in the workspace; 4s, or 10s with an action. Undo for queue edits. One at a time: a new one replaces the current.
- Contrast: text `inverse-on-surface` on `inverse-surface` 11.6:1 (Light) and 11.5:1 (Dark, Pure Black, with the `inverse-on-surface` fix); action label `inverse-surface` on `inverse-primary` 7.7:1 / 5.6:1.

### StreamInfo

Reference: `design/reference/components/StreamInfo.png`.

Technical details of what is playing, from `madrigal-player::stream_info::StreamInfo`.

- Key / value list: source (local or stream), codec, bitrate, sample rate, quality, route (`builtin`, `cache`, `local`), buffer health.
- Keys `body-medium` `on-surface-variant`, values right-aligned with tabular figures, `outline-variant` dividers.
- **Proposal:** buffer health as a small `primary` bar beside its percentage; dialog shape as in Dialog.
- Shows `stream.unknown` / an empty state when nothing is loaded. Slint: `StreamInfoPanel` in `player/dialogs.slint`.

### TempoPitch

Reference: `design/reference/components/TempoPitch.png`.

Speed and pitch for the current track (`audio.speed`, `audio.pitch`).

- Two expressive sliders with the live value in `primary` on the label row and min / neutral / max ticks under each.
- Cancel and Reset (`audio.reset`); values apply live while dragging.
- Slint: `TempoPitchDialog` in `player/dialogs.slint`.

### Tooltip

Reference: `design/reference/components/Tooltip.png`.

Plain and rich tooltips.

- Plain: `inverse-surface` / `inverse-on-surface`, `body-small`, `radius-extra-small`, beside the control. Used by the collapsed sidebar rail (`tip-text`) and icon-only buttons.
- **Proposal:** rich tooltip for settings that need a sentence: `surface-container`, `radius-medium`, title `title-small`, body `body-medium`, an optional text action.
- Appears after 500ms hover or on keyboard focus.

## Motion

### MotionCurrent

Reference: `design/reference/components/MotionCurrent.png`.

The motion Madrigal ships today, looped for reference.

- Expressive feedback is shape and colour, never scale: Slint has no GPU scale primitive.
- Easing: ease-in-out for state changes, ease-out for lifts, ease for the lyric scroll.
- Every loop stops under `prefers-reduced-motion`.

### MotionProposed

Reference: `design/reference/components/MotionProposed.png`.

Intentional additions: M3 Expressive motion Madrigal does not ship yet, drawn here as the spec for later Slint work.

- Each stays inside the Slint limits: width, radius, colour, offset and weight animate; nothing scales.
- Spring overshoot can be approximated with Slint's `ease-out-back` or a `cubic-bezier`.
- All of them stop under reduced motion; a settings toggle should do the same in the app.

## Screens

### CompactShell

Reference: `design/reference/screens/640x480/` and `1366x768/` show the compact layout.

The shell at 800×450 (16:9): what the breakpoints do together.

- Under 1100px the sidebar becomes the 80px icon rail (labels move to tooltips).
- Under 840px the player bar goes compact: position, duration and volume drop; **proposal:** a 3px progress line along its bottom edge keeps position visible.
- Under 800px the queue drawer opens as a full-width sheet; page titles step down to `headline-large`.

### DetailScreen

Reference: `design/reference/screens/1920x1080/detail-dark.png` (and other sizes and themes).

Album detail at 1280×720, opened from Home. Stacks over any page (`detail-stack`), so the sidebar keeps the page it came from.

- Back text button, DetailHeader, then a numbered track list with a hairline column header (#, Title, duration).
- The playing track swaps its number for the PlayingIndicator and takes `primary-container`.
- Loading, empty and error states use `common.loading`, `detail.empty`, `detail.error` with Retry.

### DownloadsScreen

Reference: `design/reference/screens/1920x1080/downloads-dark.png` (and other sizes and themes).

Downloads at 1280×720. **Redesign proposal.**

- Display title with Pause all (outlined) and Clear finished (text).
- **Proposal:** a summary strip of filter pills with counts (All, Active, Done, Failed) and a storage meter on the right, so the page answers "how much is left and how much space is used" at a glance.
- Rows as in DownloadRow; below 700px of page width progress and speed hide.

### ExploreScreen

Reference: `design/reference/screens/1920x1080/explore-dark.png` (and other sizes and themes).

Explore at 1280×720. **Redesign proposal.**

- Display title and the `explore.description` line; moods and genres as the 4-column MoodTile grid (row-major: Chill, Christmas, Commute, Energize ...).
- **Proposal:** charts as a numbered list (rank in `title-large`, top three in `primary`, a small up / same / down mark) next to the New releases rail, instead of a second card rail.
- Video releases (`catalog.releases.videos`) follow as another rail.

### FullPlayer

Reference: `design/reference/screens/1920x1080/full-player-dark.png` (and other sizes and themes).

The full-screen player: console on the left, a pane with Lyrics / Up next / Details on the right. **Redesign**; interactive version in the prototype, PNGs in `design/reference/screens/*/full-player-*.png`.

**Colour from the artwork.** The player takes a `tint` from the cover (matugen seam; until then the album's accent). Ground = `tint` 14% into `surface`, pane = `tint` 10% into `surface-container`, cover shadow in `tint`. Background styles: Theme (flat tint), Gradient (tint fading down), Blur, Glow (radial light behind the cover).

**Header (64px).** Close (chevron-down), a "Playing from <source>" chip, Share lyrics as image, More (TrackMenu).

**Console.**
- Cover up to 480px, `radius-extra-extra-large` (48) breathing to 32 while playing; size follows the window height (`min(100%, 480, 100vh − 600)`), never below 160px.
- Title `display-small` 36/500, then artist (a link to the artist) · album · year in `body-large`; a 48px like toggle that morphs disc → `radius-large` square and fills `primary` when liked.
- Seek: wavy while playing (fixed 24px wavelength, clipped at the handle, never stretched), flat when paused; elapsed and remaining time under it in tabular figures.
- Transport by size: 48px shuffle and repeat toggles (selected = `secondary-container`, `radius-medium`, a dot under the icon; repeat cycles off / all / one), 64px previous / next on an 8% tint, play/pause 112×80 pill that tightens to a 96px `radius-extra-large` square while playing (300ms overshoot easing).
- Volume: an 8px track with a 4px handle and the value in digits.
- Up next card: the next song; opens the Up next tab.

**Pane.** Pill tab switcher (Lyrics, Up next, Details), then:
- Lyrics: a Synced chip and the −0.1 s / offset / +0.1 s control; lines in Figtree; the active line 12px larger and bold, with a karaoke sweep: sung words `on-surface`, the current word fills `primary` left to right, unsung words `on-surface` at 55% (3.8:1 minimum on every tint and theme); translation under it in `body-large`; past lines at 0.8. Tap a line to seek.
- Up next: 60px rows with jump and remove.
- Details: title, album, source, codec / bitrate / sample rate, route, loudness, buffer health.
- Tool pills at the bottom: Sleep timer (shows the time when set), speed (1.00×), equalizer preset; each opens its dialog.

**Sizes.** Two columns from 1100px wide and 700px tall; below, one scrolling column. Under 600px tall the cover shrinks to a 72px thumbnail beside the title so transport stays above the fold (640×480 reference).

**Contrast** (all tints × Light / Dark / Pure Black): title 13:1, secondary text 7.3:1, active-line unsung words 3.8:1 (large bold), sung word 4.1:1, play icon 5.3:1, cover glyph 3.2:1.

Slint: `FullPlayer` / `PlayerConsole` / `PlayerTabs` / `PlayerTools` in `player/full.slint`, `LyricsView` in `player/lyrics.slint`. The wave needs a clipped `Path` of fixed wavelength; the play morph animates width and border-radius only.

### HomeScreen

Reference: `design/reference/screens/1920x1080/home-dark.png` (and other sizes and themes).

Home at 1280×720 (16:9; the reference set also renders 1920×1080): how the parts compose. **Redesign proposal.**

- Page title `display-medium-emphasized` (45/500) instead of a headline, the one big type moment per page.
- Hero row: a "Continue listening" `primary-container` block at `radius-extra-large` with a 64px `radius-large` play button, beside four shortcut tiles that each take a different expressive corner set (pill-topped, one oversized corner). Shape variety lives in this row only; lists below stay calm.
- Quick picks as a 3-column grid of 54px rows; albums as a 150×190 card rail.
- Sidebar `surface-container-low`, player bar floating at the bottom, page margin 16/24, section gap 24.

### LibraryScreen

Reference: `design/reference/screens/1920x1080/library-dark.png` (and other sizes and themes).

Library, Songs tab, with two rows selected. **Redesign proposal.**

- Display title with a tonal "New playlist"; primary Tabs; tag chips with the sort on the right.
- Multiselect: each row shows a checkbox; selected rows take `surface-container-highest` plus the checked box, and the SelectionBar replaces the toolbar while a selection exists.
- The playing row keeps `primary-container` and the PlayingIndicator. Selected rows never use `secondary-container`: in Light it is #cfe5ff against `primary-container` #d6e7ff, too close to tell "selected" from "playing".

### QueueDrawer

Reference: `design/reference/screens/1920x1080/queue-dark.png` (and other sizes and themes).

The queue as a 380px side sheet that becomes full width below 800px and always stops above the 96px player bar.

- `surface-container-low` sheet, `radius-extra-large` on the leading corners only (proposal; current code is square).
- Header `title-large` with the count, lock and close icon buttons; tabs All / Results / Up next / Recommended as 36px pills.
- **Proposal:** section labels *Now playing* and *Up next* (`label-medium`), and the current track as a `primary-container` row whose cover becomes a `primary` tile with the animated PlayingIndicator.
- Rows 64px, 48px cover, title `body-large`, artist `body-medium`, grip handle for drag reorder (commits on pointer up), More actions.

### SearchScreen

Reference: `design/reference/screens/1920x1080/search-dark.png` (and other sizes and themes).

Search at 1280×720 with results for a query. **Redesign proposal.**

- Search bar (focused, 2px `primary`) and the Online / Local source switch on one toolbar; filter chips All / Songs / Albums / Artists / Playlists beneath.
- Results: a top-result panel (`surface-container`, `radius-extra-large`, 96px art, Play + Shuffle) beside the first songs, then album and artist rails with "See all" and scroll arrows.
- Empty, error and offline states use EmptyState with `search.empty.*`, `search.error.network` and Retry.
- Below 420px of page height the hint and suggestions title hide.

### SettingsScreen

Reference: `design/reference/screens/1920x1080/settings-dark.png` (and other sizes and themes).

Settings at 1280×720: group list plus the selected section.

- 232px group list (Appearance, Content, Player, Lyrics, Internet, Storage, Privacy, Account, Integrations, AI, Updates) as 48px pills, active `secondary-container`.
- The section is a card of SettingsRows with a "Show advanced" switch at the end.
- Below 860px of page width the group list becomes a horizontal chip strip above the section.
