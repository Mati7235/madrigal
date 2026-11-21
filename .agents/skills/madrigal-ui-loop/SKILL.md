---
name: madrigal-ui-loop
description: "Drive the real Madrigal app to verify UI work: launch it in a headless sway (Wayland, tiling), inspect and operate it through Slint's embedded MCP server (element tree, click, type, screenshots), and check window behavior (resize, tiling, app_id) through sway. Use after any change in crates/madrigal-ui or crates/madrigal-app that affects what the user sees or does, and whenever asked to test, click through, reproduce a UI bug, or screenshot the app."
---

# Madrigal UI loop

The checks in `cargo test -p madrigal-ui` lint geometry and render PNGs, but they never run the real window inside a real compositor. This loop does. Use both: the checks for regressions, the loop to prove the change works for a user.

## Two channels, one rule

- **Slint MCP** (`scripts/mcp`): semantic. Find elements by id, accessible role or label; click, type, scroll, set values; screenshot the window as Slint renders it. Use it for everything inside the window.
- **sway** (`scripts/ui`): the compositor. Window size, tiling vs floating, app_id, output resolution, and real pointer input through a virtual pointer. Use it only for what MCP cannot see: how the window behaves in a window manager, and a final check that a real click lands.

Rule: never guess pixel coordinates when an element handle exists. Coordinates from a screenshot are a last resort, and only after `ui window` tells you where the window is.

## Setup

Build first (repo build rules apply: `CARGO_TARGET_DIR` on the SSD, `flock`, `CARGO_BUILD_JOBS=12`):

```sh
cargo build --release -p madrigal-app --bin madrigal
S=.agents/skills/madrigal-ui-loop/scripts
$S/session start          # headless sway + virtual pointer; reuses a live session
$S/session app            # (re)launches the app with SLINT_MCP_PORT; prints pid, port, log
$S/session status
```

State lives in `$CARGO_TARGET_DIR/ui-loop/` (override with `MADRIGAL_UI_LOOP_DIR`; app profile in `profile/`, log in `app.log`). Port: `SLINT_MCP_PORT`, default 8590. Run `session app` again after every rebuild; it replaces only the app it started. Needs `sway`, `grim`, `wtype`, `jq`, `curl`; Xwayland errors in `sway.log` are expected.

When opencode starts while the app is running, the same server is also available as the `madrigal-ui` MCP tools (`opencode.json`). `scripts/mcp` works either way.

## The loop

One action, then look. Do not chain actions blind.

1. **Observe.** `$S/mcp get_window_properties` (size, root handle), then `$S/mcp get_element_tree '{"elementHandle":ROOT,"maxElements":60}'` or a query (below). Take `$S/mcp take_screenshot '{"imageMimeType":"image/png"}' FILE` and open the PNG.
2. **Act** on one element by handle: `click_element`, `dispatch_key_event`, `scroll_element`, `set_element_value`, `drag_element`.
3. **Verify** the effect: re-query the element or its page, and screenshot again. A `{}` reply only means the event was dispatched, not that the app did what you wanted.
4. If the result is wrong, read `app.log`, form a hypothesis, fix code, rebuild, `session app`, and repeat from 1. Never retry the same action hoping for a different result.

Network actions (search, artwork, streams) are asynchronous: after acting, wait a few seconds, then observe again; poll up to ~15 s before calling it broken.

## MCP recipes

Handles are `{"index":"N","generation":"G"}`; window and element handles are different kinds. `scripts/mcp TOOL [ARGS] [PNG]` fills `windowHandle` for tools that need it and writes screenshots to a file (default `ui-loop/mcp-shot.png`).

```sh
$S/mcp get_window_properties
$S/mcp find_elements_by_id '{"elementsId":"MainShell::nowbar"}'
$S/mcp query_element_descendants '{"elementHandle":ROOT,"findAll":true,"queryStack":[{"matchDescendants":true},{"matchElementAccessibleRole":"TextInput"}]}'
$S/mcp query_element_descendants '{"elementHandle":ROOT,"queryStack":[{"matchDescendants":true},{"matchElementAccessibleLabel":"Buscar"}]}'
$S/mcp get_element_properties '{"elementHandle":H}'
$S/mcp click_element '{"elementHandle":H}'
$S/mcp dispatch_key_event '{"text":"the beatles","eventType":"PressAndRelease"}'
$S/mcp dispatch_key_event '{"text":"\n","eventType":"PressAndRelease"}'
$S/mcp scroll_element '{"elementHandle":H,"deltaY":-400}'
$S/mcp take_screenshot '{"imageMimeType":"image/png"}' /path/shot.png
```

Ids are `Component::id` from the .slint sources (`grep -rn "id-name :=" crates/madrigal-ui/ui`). Roles: `Button`, `TextInput`, `Text`, `List`, `ListItem`, `Tab`, `Switch`, `Slider`, ... Labels are the visible, localized strings (the app runs in Spanish).

## sway recipes

```sh
$S/ui window                                  # app_id, floating, rect, geometry
$S/session output 1920x1080                   # simulate another screen; re-sizes the pointer too
$S/ui sway '[pid=PID] floating enable, resize set 683 768'
$S/ui sway '[pid=PID] floating disable'
$S/ui shot FILE                               # full output as the compositor shows it
$S/ui click X Y | move X Y | scroll DY | type TEXT | key Return
```

Output coordinates are absolute; the window starts at `rect` from `ui window` and has a 2px border. After a resize, compare `get_window_properties.size` (what Slint thinks) with `ui window` geometry (what sway gives): they must match.

## What to check for UI changes

- At 1920x1080, 1366x768, 1280x720 and a 683x768 / 640x480 tile: nothing cropped, the player bar fully visible, no horizontal overflow, text elides instead of clipping, inputs reachable and typing lands where the caret is.
- The window is tiled by default, resizable, `app_id` is `madrigal`.
- The flow the change is for works end to end through MCP, and once with a real `ui click` on the main control.
- Judge screenshots with the design rules in force (`madrigal-ux`, and `impeccable` craft floor when it is in scope). The software renderer in sway shows small artifacts; MCP screenshots are clean and are the reference for visuals.

## Pitfalls

- `query_element_descendants` with `findAll` can return the same handle several times; deduplicate.
- A floating window keeps its size when the output changes; a fixed-size window (min = max) is always floating in sway. If `ui window` says `floating: true` right after launch, the window is not resizable.
- `session stop` ends sway too. Do not stop a session someone else is using; `session status` first.
- The real pointer only exists while `vptr` runs (`session start` launches it). The sway seat has no pointer of its own in headless mode, so `swaymsg seat cursor` and one-shot tools like `wlrctl` do not deliver clicks.
- No secrets in screenshots or logs you commit: never sign in with a real account inside the loop profile.
