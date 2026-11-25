Madrigal is a Rust desktop music player (local library and YouTube Music, synced lyrics, scrobbling, MPRIS) whose interface is built in Slint on **Material 3 Expressive**. This system is the web mirror of `crates/madrigal-ui/ui/tokens.slint` and `ui/components/`; the Slint files stay the source of truth, and `docs/UI.md` holds the direction.

## Principles

- **Music first, chrome second.** Surfaces are quiet tonal steps (`surface` → `surface-container-highest`); the only loud colour is `primary`, kept for the thing that is playing or the action that plays it.
- **Expressive through shape and motion, not decoration.** Corners morph on press (`radius-medium` 12 → `radius-small` 8 in `duration-pressed` 150ms), selected icon buttons go from a disc to a rounded square, switch thumbs grow 16 → 24px. No gradients, no scale transforms.
- **Dense where it scans, roomy where it shows.** Track rows are 54px; media cards are 150×190 with 118px art; sections breathe at `space-24`.
- **Keyboard is a first-class pointer.** Every control is Tab reachable and draws a 2px focus ring in its content colour, inset 2px.
- **Two type voices.** Figtree speaks (titles, buttons, nav, tabs, the active lyric line); Inter reads (track lists, subtitles, the rest of the lyrics).

## Themes

| Theme | Ground | Accent | Notes |
| --- | --- | --- | --- |
| Light | `surface` #ffffff | `primary` #0b6bcb sky blue | containers are cool blue tints (#f4f8fe → #d8e3f2) |
| Dark | `surface` #0b0810 | `primary` #cfbcff violet | containers are violet-black (#120e19 → #292234) |
| Pure Black | `surface` #000000 | `primary` #cfbcff violet | Dark roles with the surface ramp shifted to #000000 for OLED |

`appearance.theme` picks the mode; `pure-black` swaps the Dark ramp. A matugen-derived palette (from the wallpaper) for the System mode is a planned runtime seam that fills the same 48 roles, so components must only ever reference roles, never hex.

## Colour rules

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

## State layers

Every interactive surface draws a layer of its **content colour** over its fill: `state-hover` 0.08, `state-pressed` 0.1, media card hover `state-media-hover` 0.14. Disabled: fill = `on-surface` at `state-disabled-container` 0.12, content = `on-surface` at `state-disabled-content` 0.38. Layers fade in `duration-hover` 100ms ease-in-out.

## Type

30 styles: the M3 scale (display, headline, title, body, label × large/medium/small) plus an *emphasized* twin of each (weight 500 or 700). Display, headline, title and label large/medium are Figtree; body and `label-small` are Inter. Pages never name a font: they pick a style.

- Row title: `title-small` 14/500; row subtitle and duration: `label-small` 11px.
- Media card title `title-small`-weight 600 at 13px, subtitle 11px (density spec in `docs/UI.md`).
- Buttons, chips, tabs, menu rows: `label-large` 14/500.
- Line heights follow the M3 baseline scale (Slint tokens carry size, weight and tracking only).

## Shape

`radius-extra-small` 4 · `small` 8 · `medium` 12 · `large` 16 · `large-increased` 20 · `extra-large` 28 · `extra-large-increased` 32 · `extra-extra-large` 48 · `full` (pill).

Small buttons rest at 12 and press to 8; medium buttons rest at 16 and press to 12. Rows and menu rows are 8. Media cards, mood tiles and the track menu are 12. Chips, switches, search bar, segmented buttons and icon buttons are pills.

## Spacing and layout

Seven steps: 4 · 8 · 12 · 16 · 24 · 32 · 48. Page margin 16px from the sidebar and the workspace top; section gap 24; card rails gap 12.

Reference resolutions are 16:9: 1920×1080 first, then 1366×768 and 1280×720, plus a 640×480 tile for the minimum. Every screen must fit them with the player bar fully visible and no horizontal overflow.

Shell: 244px sidebar (48px items, 16 padding, 6 gap) that collapses to an 80px rail under 1100px; 96px Now Playing bar always visible; 380px queue drawer that becomes a full-width sheet under 800px. Breakpoints follow content, not devices (full table in `docs/UI.md`).

## Elevation and motion

Elevation is used sparingly: elevated cards (`shadow-card`, lifting to `shadow-card-hover` on hover in `duration-fade` 200ms ease-out) and the track menu (`shadow-menu`). Everything else is separated by tonal surface steps.

Motion: hover 100ms, press morph 150ms, fades 200ms, lyric auto-scroll 300ms ease. The full M3 duration scale (50ms → 1000ms) is in tokens. Expressive feedback is shape and colour, never scale (Slint has no scale primitive): buttons morph 12 → 8 on press, the play button is a disc when paused and a `radius-small` square while playing, switch thumbs grow 16 → 24px. The **MotionCurrent** card loops each of these.

### Intentional additions

**MotionProposed** specifies M3 Expressive motion not yet in the app, each within Slint's limits: a wavy seek bar while playing, a connected transport group where the pressed button widens, a nav pill that stretches between items, a shape-morphing loader for buffering and downloads, and a lyric line that eases colour and weight in. Approximate spring overshoot with `ease-out-back` or a `cubic-bezier`; every animation stops under reduced motion.

## Redesign proposals

Where this system goes past the current code, the card says **Proposal** or **Intentional addition**. The direction, in one line each:

- **Hierarchy by size and shape, not colour.** One big type moment per page (`display-medium-emphasized` page titles), one oversized control per surface (72px play in the full player), shape variety confined to hero rows.
- **Motion with meaning.** M3 emphasized easing for anything that moves through space (`easing-emphasized-decelerate` in, `easing-emphasized-accelerate` out), standard easing for colour and opacity; expressive spatial springs where Slint can approximate them.
- **Playing state is a glyph.** The PlayingIndicator marks the current track everywhere: rows, queue, cards. Colour alone never says "playing".
- **Feedback for silent actions.** Snackbar after queue, like and download actions; EmptyState for empty pages.
- **Softer, larger dialogs.** `radius-extra-large`, centred icon and title.
- **Screens:** every page composed at 1280×720 (HomeScreen, SearchScreen, ExploreScreen, LibraryScreen, DownloadsScreen, SettingsScreen, DetailScreen, FullPlayer, QueueDrawer) plus CompactShell at 800×450 for the breakpoints.
- **Selected is not playing.** Selected rows use `surface-container-highest` and a checked box; `secondary-container` is too close to `primary-container` in Light (#cfe5ff vs #d6e7ff) to mark selection.

## Iconography

Lucide (ISC), 24px grid, 2px stroke, round caps and joins. Drawn at 20px in buttons and 24px in menu rows, tinted with the content colour of the control (`Image.colorize` in Slint; inline SVG with `currentColor` on the web). Filled variants (`heart-filled`, `star-filled`) mean the toggled-on state. Missing artwork shows `music-2` on `surface-container-high`.

## Voice

Short, sentence case, verb first: "Add to queue", "Play next", "Back to current line", "New playlist". No exclamation marks. Labels come from `madrigal-i18n` (default Spanish); English strings here are reference.

## Not mirrored

- Components are static HTML/CSS renditions hand-written from the Slint sources in `ui/components/`; the Slint library itself cannot run on the web.
- Playfair Display is embedded by `app.slint` but no style uses it; it is left out.
- The FigCraft mockups in `design/` (paper #F4F1EA, wine accent) are a superseded direction and are not part of this system.
- Shadow values are read from the components' `drop-shadow-*` properties; the `shadow` role is #000000 in every theme.
