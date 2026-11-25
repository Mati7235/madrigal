# TextField

Text input: a filled, borderless container with its label above. **Redesign proposal**, replacing the outlined field; try typing in the preview.

- Label sits **above** the field in `label-large` (`on-surface-variant`, `primary` while focused). It never moves, shrinks or hides, so there is no notch and no jump.
- Container 56px, `surface-container-highest`, `radius-large` (16). Pill variant (`radius-full`, 28) for search and one-line queries. No border at rest; hover adds the 0.08 state layer.
- Focus: a 2px `primary` ring drawn inside the container edge, so no parent can clip it. Error: a 2px `error` ring, an `error` label, a trailing warning icon and supporting text that starts with the icon.
- Value `body-large` `on-surface`; placeholder `on-surface-variant` stays visible until you type. Supporting text and counter `body-small` 6px below.
- Accessories: 24px leading icon, a unit suffix (`s`, `kbps`, `%`), a clear button once there is a value, Show / Hide for secrets.
- Contrast: with no border, the field is told from the page by its fill plus its visible label and placeholder (`on-surface-variant` is 7.17:1 on the container in Light). Do not drop both the label and the placeholder.
- Slint: `M3TextField` in `components/fields.slint`. Today it is outlined with an in-box floating label; the port drops `line` / `focus-line` for `fill` (`surface-container-highest`) and a `ring` colour, adds `shape` (large / full), `leading-icon`, `suffix`, `max-length`, `clearable`, `secret`.
