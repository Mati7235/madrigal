# Snackbar

Short confirmation after an action, with at most one action. **Intentional addition**: Madrigal has no toast today, and track actions (play next, add to queue, like, download) finish silently.

- **Expressive shape:** one-line snackbars are full pills (56px, `radius-full`); two-line ones use `radius-large-increased` (20) with the action under the text. No square corners.
- `inverse-surface` / `inverse-on-surface`, a 24px leading icon in `inverse-primary` that says what happened, the action as a filled `inverse-primary` pill, an optional dismiss button. `shadow-menu`.
- Enters by rising 16px with a fade in 300ms `easing-emphasized-decelerate`; leaves with `easing-emphasized-accelerate`. Nothing scales.
- Sits 16px above the player bar, centred in the workspace; 4s, or 10s with an action. Undo for queue edits. One at a time: a new one replaces the current.
- Contrast: text `inverse-on-surface` on `inverse-surface` 11.6:1 (Light) and 11.5:1 (Dark, Pure Black, with the `inverse-on-surface` fix); action label `inverse-surface` on `inverse-primary` 7.7:1 / 5.6:1.
