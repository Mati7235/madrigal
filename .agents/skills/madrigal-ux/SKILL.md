---
name: madrigal-ux
description: UX design and review for the Madrigal UI (Slint, Material 3 Expressive). Use when designing, changing or reviewing a screen, component, flow, empty/error state, copy or accessibility in crates/madrigal-ui. Adapted from the bmad-agent-ux-designer skill to this repo's rules.
---

# Madrigal UX

Adapted from `bmad-agent-ux-designer` (persona "Sally") to Madrigal. Not a BMAD
project: there is no `_bmad/` folder and no BMAD scripts. This skill is a
checklist and a workflow, nothing else.

## Principles

1. Every decision serves a concrete user task. Name the task before the pixels.
2. Start simple. Ship the smallest screen that completes the task, then refine from the PNG.
3. The user's time and attention come first: fewer taps, no surprise state, no dead ends.
4. Accessible by default: keyboard reachable, labeled, legible, honest focus.
5. Consistency beats novelty. Reuse M3E tokens and components before inventing.

## Sources of truth

- `docs/UI.md`: direction, decisions, screen ownership, density rules.
- `docs/A11Y.md`: accessibility audit of the Slint UI (known gaps, Slint 1.19 facts).
- `crates/madrigal-ui/ui/tokens.slint`: colors, type, shape, motion, spacing.
- `crates/madrigal-ui/ui/components/`: M3 components. Use them before writing new ones.
- `docs/THEMES.md`, `docs/LYRICS.md`, `docs/ARTWORK.md`: domain rules for those surfaces.
- The FigCraft mockups in `design/` and the Python QML oracle are references only.

## Workflow

### 1. Frame the task
- Write one sentence: who does what, on which screen, and what "done" looks like.
- List the states: default, loading, empty, error, offline, and the keyboard path.
- Check what already exists in `ui/pages/*` and `ui/shell/*` before designing anything new.

### 2. Design with the system
- Colors come from `Palette` roles only. No hardcoded hex in pages.
- Type from `Type.*` styles. Shape from `Shape.*`. Motion from `Motion.*`.
- Touch targets: 48px minimum for primary actions, 40px for dense icon buttons.
- Density follows `docs/UI.md` (list rows 54px, media cards 150x190).
- If a value must deviate from tokens, record the reason in `docs/UI.md`, not in code comments.

### 3. Accessibility checklist
- Every interactive element is keyboard reachable (`FocusScope` or `TextInput`) and shows a focus indicator.
- Every control has `accessible-label` and `accessible-role`. Decorative icons and text are silenced.
- Text contrast meets WCAG AA in all three themes (Light, Dark, PureBlack).
- Nothing is conveyed by color alone (active state also has shape, weight or a mark).
- Check the open items in `docs/A11Y.md` that touch the screen you change. Close what you fix.

### 4. Copy
- Spanish is the default; every new key needs an English entry in `madrigal-i18n` (parity is checked).
- Use the unslop rules: no em dashes, no colon connectors, no rule-of-three slogans, no emojis.
- Short labels, verbs for actions, no "Por favor" or filler.
- Error copy says what happened and what the user can do next.

### 5. Verify visually
- Use the targeted check loop, never the full gate:
  `flock /tmp/opencode/madrigal-build.lock env CARGO_BUILD_JOBS=12 cargo test -p madrigal-ui --test check_ui_<name>`
- Export PNGs with `MADRIGAL_SNAPSHOT_DIR=<scratch dir>` and read them at 1920x1080 (the default shell size).
- Self-review the PNG against the checklist above. Report PASS or FAIL with the specific items.
- A screen is not done while its PNG has visible misalignment, clipped text or empty dead space that the layout could fill.

### 6. Hand off
- Update `docs/UI.md` for any decision that changes direction, tokens or screen ownership.
- Update `docs/A11Y.md` when a listed gap is closed.
- Do not commit from this skill. Commits follow the repo's workflow (English, imperative, 50 chars max).

## Output

When reviewing, answer in this order:
1. The user task and the states checked.
2. Findings, most user-impacting first, each with the file and element.
3. Changes made or proposed, with reasons tied to the principles.
4. Verification: the check run, the PNG path, PASS or FAIL.
