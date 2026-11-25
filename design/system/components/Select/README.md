# Select

Dropdown: a tonal pill that opens a floating menu. **Redesign proposal**, replacing the field-shaped picker.

- Trigger: 48px pill (`radius-full`) in `surface-container-highest` with the value in `title-medium` and a chevron that turns 180° when open; while open it switches to `secondary-container`. Dense 40px pill in settings rows. The label, when needed, sits above like TextField.
- Menu: detached 8px below the trigger, `surface-container-high`, `radius-large-increased` (20), 8px inner padding, `shadow-menu`; enters with a 4px drop and fade in 200ms `easing-emphasized-decelerate`. Aligns to the trigger's start edge (end edge when near the window edge).
- Options: 48px minimum, `radius-medium` (12) inside the menu, a 24px leading slot, title `body-large`, optional description `body-small` in `on-surface-variant`. The chosen option shows a `primary` check in the leading slot; colour never marks it alone.
- Hover adds the 0.08 layer; keyboard focus draws a 2px `on-surface` ring around the option (shown on "Automatic").
- Unavailable options stay listed at 0.38 with a reason in the description. A 1px `outline-variant` divider separates groups; a trailing chevron opens a submenu.
- Only one menu is ever open: opening one closes any other; a click outside or Esc closes it and returns focus to its trigger.
- Keyboard: Enter / Space / Down open, Up / Down move, Enter picks, Esc closes; `aria-haspopup="listbox"`, `aria-expanded`, `aria-selected`.
