# Matugen → Madrigal palette plan

Read-only investigation. No repo files were modified, no `cargo` was run.

## 1. Matugen on this machine

| Item | Result |
| --- | --- |
| Binary | `command -v matugen` → `/usr/bin/matugen`; `matugen --version` → `matugen 4.2.0` |
| Config | `~/.config/matugen/config.toml` (401 B) |
| Templates | `~/.config/matugen/templates/p10k.zsh`, `.../papirus-folders.json` |
| `~/.cache/matugen` | does not exist |
| `~/.local/share/matugen` | does not exist |

`config.toml` has **no JSON template**; both templates emit shell/JSON fragments consumed by DankMaterialShell (DMS):

```toml
[templates.p10k]
input_path = '/home/matias/.config/matugen/templates/p10k.zsh'
output_path = '/home/matias/.config/p10k/dms-colors.zsh'

[templates.papirus-folders]
input_path = '/home/matias/.config/matugen/templates/papirus-folders.json'
output_path = '/home/matias/.cache/DankMaterialShell/papirus-folders.json'
post_hook = '/usr/bin/python3 /home/matias/.local/bin/dms-papirus-recolor'
```

Template syntax is `{{colors.<role_snake_case>.default.hex}}` and `{{mode}}` (see `templates/p10k.zsh`, `templates/papirus-folders.json`). So the hook mechanism exists and is already in use.

## 2. Real output formats

### 2a. Live file written by DMS (what to read)

`/home/matias/.cache/DankMaterialShell/dms-colors.json` — 5634 B, mtime `2026-11-18 23:32:02`, rewritten whenever the wallpaper changes (`~/.cache/DankMaterialShell/matugen-seeds.json` holds `seed` + `used` timestamps per wallpaper, e.g. `"used":1795055522398578887` matching that mtime).

Shape (verified with `python3 -c` over the file):

```json
{"mode":"dark",
 "dank16":{"color0":{"dark":"#1d100a","light":"#fff8f6","default":"#1d100a"}, ...},
 "colors":{"light":{"primary":"#a04100","on_primary":"#ffffff","surface":"#fff8f6", ...},
           "dark" :{"primary":"#ffb693","on_primary":"#562000","surface":"#1d100a", ...}}}
```

* `colors` holds **50 M3 roles**, each a plain `#rrggbb` string, per mode.
* Values are produced with `scheme-vibrant` (`~/.config/dms/settings.json`: `"matugenScheme":"scheme-vibrant"`, `"currentThemeName":"dynamic"`, `"matugenSpec":"2021"`).

### 2b. Raw matugen JSON (reference / fallback)

`matugen color hex '#6750A4' --json hex --dry-run` (dry-run: writes nothing) produced `/tmp/opencode/matugen-sample.json`:

```json
{"base16":{...},
 "colors":{"primary":{"dark":{"color":"#cfbdfe"},"default":{"color":"#cfbdfe"},"light":{"color":"#65558f"}}, ...},
 "image":"", "is_dark_mode":true, "mode":"dark", "palettes":{...}}
```

Top-level keys: `base16`, `colors`, `image`, `is_dark_mode`, `mode`, `palettes`. Each role carries **`light` and `dark` simultaneously**, so one cheap invocation fills both globals. `default` follows `-m` (default `dark`). Note this sample used `scheme-tonal-spot` (the `--type` default), unlike DMS's `scheme-vibrant`.

## 3. Mapping to `crates/madrigal-ui/ui/tokens.slint`

`tokens.slint` `export struct Palette` has **48 fields**. Mechanically compared against `matugen-sample.json["colors"]`:

```
repo fields: 48
missing in matugen: []
matugen extra: ['on_background', 'source_color']
```

So the mapping is **identity**: kebab-case → snake_case, 1:1, nothing to derive.

| Madrigal `Palette` field | matugen key | Light (live) | Dark (live) |
| --- | --- | --- | --- |
| `primary` | `primary` | `#a04100` | `#ffb693` |
| `on-primary` / `on_primary` | `on_primary` | `#ffffff` | `#562000` |
| `primary-container` | `primary_container` | — | — |
| `secondary-container` | `secondary_container` | — | — |
| `on-secondary-container` | `on_secondary_container` | — | — |
| `surface` | `surface` | `#fff8f6` | `#1d100a` |
| `surface-container` (…-high/-highest/-low/-lowest, `surface-bright`, `surface-dim`, `surface-variant`, `surface-tint`) | same names | `#ffeae1` | `#2b1c15` |
| `on-surface` / `on-surface-variant` | same | — | — |
| `outline` / `outline-variant` | same | `#887369` | `#a38c82` |
| `error`, `on-error`, `error-container`, `on-error-container` | same | `#ba1a1a` | `#ffb4ab` |
| `background` | `background` (M3-legacy, still emitted) | — | — |
| `scrim`, `shadow` | same | `#000000` | `#000000` |
| `*_fixed`, `*_fixed_dim` | `primary_fixed`, `primary_fixed_dim`, … | — | — |

Policy for gaps (defensive, since format is third-party):

1. Role present → use matugen value.
2. Role absent or unparsable hex → fall back to the **static value of the same role in `Light.palette` / `Dark.palette`** (already the oracle tables in `tokens.slint`), not to a neighbouring role.
3. `on_background` / `source_color` are ignored (no matching field).
4. Fewer than ~40 roles parsed, or no `light`/`dark` map at all → `load()` returns `None` and the static palettes stay untouched (all-or-nothing guard, avoids half-themed UI).

`PureBlack` stays manual (matugen has no pure-black variant); for it, reuse `Dark` mapped values with `surface*` overridden as today.

## 4. Mode detection and refresh

* Mode source of truth: top-level `"mode":"dark"|"light"` in `dms-colors.json`; raw matugen JSON also exposes `"is_dark_mode": bool`. Unknown/missing → treat as dark (matches current `ThemeKind` default).
* Do **not** infer mode from `madrigal_settings::DarkMode`: `crates/madrigal-ui/src/theme.rs` currently resolves `Theme::Matugen => ThemeKind::Dark` and there is no system-theme reader anywhere in the workspace (`grep -rn "color-scheme|org.freedesktop.appearance" crates` → nothing).
* Refresh: no watcher crate is available — `Cargo.lock` has no `notify` (only the unrelated `madrigal-notify` crate), and `[workspace.dependencies]` does not exist. Use a cheap mtime poll (e.g. 2 s, or piggyback on an existing UI timer): `fs::metadata(path)?.modified()` change → re-`load()` → re-apply globals.
* Robustness: the file is produced by a template write from another process; on `serde_json` error, retry once after the file's mtime settles, then keep the previous palette. Never leave a partially applied palette.
* Alternatively, add our own template entry to `~/.config/matugen/config.toml` with `output_path` pointing at a private JSON and a `post_hook`; that edits user config, so it is opt-in and out of scope for the first cut. Reading `dms-colors.json` requires no config change at all.

## 5. Proposed integration

`Theme::Matugen` already exists (`crates/madrigal-settings/src/appearance.rs:7-13`) and is already advertised in `docs/UI.md` ("matugen-derived colors for the system mode are an F9 runtime seam").

### 5a. New crate `crates/madrigal-matugen` (no Slint, no Qt)

```rust
pub const ROLE_COUNT: usize = 48;
pub const ROLES: [&str; ROLE_COUNT] = ["background", "error", "error_container", /* … tokens.slint order … */];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }

pub struct Palette([Rgb; ROLE_COUNT]);
impl Palette { pub fn get(&self, role: usize) -> Rgb { self.0[role] } }

pub struct Theme { pub light: Palette, pub dark: Palette, pub dark_mode: bool }

pub fn load(path: &Path) -> Option<Theme>;
```

`load` accepts both shapes:

* DMS: `{"mode": "...", "colors": {"light": {role: "#rrggbb"}, "dark": {...}}}`
* raw: `{"mode": "...", "colors": {role: {"light": {"color": "#…"}, "dark": {"color": "#…"}}}}`

Steps: read bytes → `serde_json::Value` → detect shape → for each of the 48 `ROLES`, look up the mode map → parse `#rrggbb` (also accept `#rgb`) → fill; count failures; if `< 40` successes for a mode, return `None`.

Register `[[check]] name = "matugen_palette" group = "offline"` in `checks.toml`, check at `crates/madrigal-matugen/tests/checks/check_matugen_palette.rs` with two fixtures under `tests/fixtures/` (`dms-colors.json`, `matugen-raw.json`) copied from the verified shapes above, plus a truncated file (→ `None`) and a file with one missing role (→ that role keeps fallback).

### 5b. `madrigal-ui` side

`crates/madrigal-ui/ui/app.slint` currently exports `DemoGallery`, `MainShell`, `I18n`, catalog types — **not** the palette globals. Slint requires the re-export for Rust access (docs, Globals 1.19: *"To reach a global from your business logic in Rust … re-export it from the file that also exports your main application component"*, then `app.global::<Logic>().set_the_value(42)`).

So:

```slint
export { Light, Dark, PureBlack } from "tokens.slint";
```

Then in `crates/madrigal-ui/src/theme.rs` (or `matugen.rs`):

```rust
use slint::ComponentHandle;

pub fn apply_palette(window: &MainShell, theme: &madrigal_matugen::Theme) {
    window.global::<Light>().set_palette(to_slint(&theme.light));
    window.global::<Dark>().set_palette(to_slint(&theme.dark));
}
```

`Palette` needs no extra export to be constructible: it is already generated because `MainShell.palette` is `in-out property<Palette>`; the same pattern is used today for `LibSongRow { … }` in `crates/madrigal-ui/src/pages_library.rs:101`. Construct with `slint::Color::from_rgb_u8(r, g, b)` per field. `Light.palette` / `Dark.palette` are declared `in-out` (`tokens.slint:52-53`, `105-106`), so `set_palette` is generated.

### 5c. Consumption at start-up

`crates/madrigal-app/src/lib.rs::boot_headless` is headless today (no `MainShell::new()`, only `demo`/checks create windows). Wire it where the window is created:

1. `madrigal_app::boot_headless` returns `Boot`; add `theme: Option<Theme>` filled from `load(&dms_colors_path())`, path = `XDG_CACHE_HOME` default `~/.cache` + `/DankMaterialShell/dms-colors.json`; if `Theme` is `None` (matugen/DMS missing) keep `None` and the static palettes.
2. Window init: `apply_palette(&shell, &theme)` **before** `apply_shell_theme(...)`.
3. Kind selection: `appearance.theme == Theme::Matugen` → `kind = if theme.dark_mode { Dark } else { Light }`; change `theme.rs:41` from `Matugen => ThemeKind::Dark` to `Matugen => <resolved from matugen>` (pass a bool, e.g. `from_appearance_with(appearance, matugen_dark: Option<bool>)`).

**Ordering caveat (important):** `MainShell.apply-theme` *assigns* `root.palette = Light.palette`, which replaces the property binding (`ui/shell/shell.slint:355-362`). Therefore globals must be set before invoking `apply-theme`, and any later wallpaper refresh must set the globals **and** re-invoke `apply-theme`. Preferred follow-up refactor: make the shell reactive instead —

```slint
palette: theme-kind == 0 ? Light.palette : theme-kind == 1 ? Dark.palette : PureBlack.palette;
```

and reduce `apply-theme` to `root.theme-kind = k;` so palette updates propagate without re-invoking anything. Same change applies to `ui/gallery.slint:11,26-28`.

## 6. Risks

| Risk | Mitigation |
| --- | --- |
| Matugen/DMS absent (bare Fedora/Arch, other DE) | `load()` → `None`, static `Light`/`Dark`/`PureBlack` untouched; `Theme::Matugen` falls back to today's Dark |
| Third-party JSON shape drift (matugen 4.2.0 today, DMS-owned file) | accept both shapes, tolerate unknown keys (`serde_json::Value`, no `deny_unknown_fields`), count-check before applying |
| `dms-colors.json` written by another process while reading | parse-error retry, keep previous palette |
| Double theming source (app `pure_black` + matugen surface ramp) | `PureBlack` keeps static values, only `Light`/`Dark` globals get matugen |
| Startup cost | single 5.6 KB read + `serde_json` parse, sub-ms; do it before window show, not per-frame |
| Re-theming churn | mtime poll only (no watcher dependency), and only 2 `set_palette` calls per change |
| Wrong accent variant (DMS uses `scheme-vibrant`, matugen default is `scheme-tonal-spot`) | document that values come pre-derived by DMS; do not re-run matugen ourselves |
| Global not exported from `app.slint` | `global::<Light>()` fails to compile — add the `export { Light, Dark, PureBlack }` line (verified missing today) |
