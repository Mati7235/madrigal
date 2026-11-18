# Testing

`cargo xtask checks` is the only quality gate.

## Groups

| Group   | Covers                                                 |
| ------- | ------------------------------------------------------ |
| offline | Pure logic, no network or system binaries              |
| native  | System integration (libmpv, MPRIS, keyring)            |
| live    | Real network (YouTube, LRCLIB, SponsorBlock); opt-in    |
| visual  | UI inspection; opt-in                                  |

By default `offline` + `native` run. `live` and `visual` only on request.

## Exit codes

- `0`: all PASS or SKIP, or an empty registry (`0 checks`).
- `1`: at least one FAIL.
- `2`: invalid usage (unknown group, missing value, wrong subcommand).

Each check prints `PASS|FAIL|SKIP <name> <group>`.
SKIP = the `check_<name>` target doesn't exist yet or the environment doesn't support it.
Without `cargo` available the check is FAIL.

## Adding a check

1. Create `crates/<crate>/tests/checks/check_<name>.rs`.
2. Add the row in `checks.toml`:
   `[[check]]` + `name = "<name>"` + `group = "<group>"`.
3. Add the row to the table below.
4. The check must isolate XDG state in a temp directory and leave nothing behind.
5. Env overrides are process-global: if a check binary holds more than one test
   that mutates env vars, serialize them with a static `Mutex`.

## Registry

| Check | Group | What it verifies |
| ----- | ----- | ---------------- |
| `core_models` | offline | serde roundtrip and invariants of `madrigal-core` |
| `i18n_parity` | offline | es/en parity of madrigal-i18n: no orphans or empties, tr() non-empty, unknown key errors |
| `paths_isolation` | offline | All paths contained within MADRIGAL_PROFILE_ROOT |
| `files_atomic` | offline | All-or-nothing atomic writes with no temp leftovers |
| `secrets_permissions` | offline | Secrets 0600, symlink rejection, keyring disabled |
| `testkit_selftest` | offline | Headless UI harness: click, virtual clock, drag, PNG |
| `db_migrations` | offline | Fresh DB migrates to latest, idempotent reopen, basic song/playlist/event roundtrip |
| `db_recovery` | offline | Damaged header is quarantined with timestamp and fresh DB takes over; empty and healthy files are never quarantined |
| `streams_orchestration` | offline | yt-dlp shim: resolution, client fallback, 429 backoff |
| `streams_cache` | offline | Stream cache TTL, prune of expired only, atomic writes |
| `streams_po_token` | offline | PO token injection via local server, offline degradation |
| `settings_roundtrip` | offline | Typed settings load/save with sanitize, unknown-key retention and per-account isolation |
| `scrobble_thresholds` | offline | Scrobble eligibility table: 50%/180s cap with 30s floor, boundary cases |
| `scrobble_clients` | offline | Maloja/Last.fm/ListenBrainz payloads, auth signature, error mapping via local mocks |
| `scrobble_outbox` | offline | Persistent outbox: reload, flush, dedup, growing backoff |
| `lyrics_parsers` | offline | LRC/enhanced/TTML/SYLT parsing, markers, to_lrc roundtrip |
| `lyrics_sync_helpers` | offline | Karaoke boundary math: exact line starts/ends, word sweeps, lookahead |
| `lyrics_translation` | offline | Mock provider chain, discarded timings, per-line attach, db cache persistence |
| `lyrics_embedded_image` | offline | Embedded USLT/SYLT extraction from MP3 fixtures, share_image PNG render |
| `net_proxy` | offline | Proxy routing, auth header, stream bypass, probe, DoH construct-only |
| `net_retry` | offline | Backoff attempts, jitter bounds, budget, section counters |
| `library_playlists` | offline | Local playlists with tags, cover, hidden flag, liked auto-playlist, duplicate policy and undo |
| `library_stats` | offline | Play counts, streaks, listen time and Year in Music tops over seeded events |
| `library_history` | offline | Listen/search history with settings pause, clear and duration window |
| `account_cookies` | offline | Netscape/header cookie parsing, YouTube domain filter, roundtrip serialize, expiry, 0600 writes |
| `account_sapisid` | offline | SAPISIDHASH golden vectors and browser auth header assembly |
| `account_profiles` | offline | Isolated account profiles, settings active pointer, delete, 0600 cookies, browser seam |
| `innertube_search` | offline | Fixture search songs/artists/albums map to core models, filter mapping, suggestions |
| `innertube_pages` | offline | Fixture artist/album/playlist/watch pages map to models, moods via custom parse, malformed input errors |
| `localmedia_scan` | native | Local folder scan (ffmpeg fixtures): tags/durations/artwork/lyrics, exclude/short/corrupt handling, Local db upserts, artwork prune |
| `downloads_queue` | offline | Download queue: concurrency cap, pause/resume, cancel, retry, quarantine, delete, layout, export/import |
| `downloads_postprocess` | native | ffmpeg segment cut with chapter rewrite on real audio |
| `backup_restore` | offline | Full backup snapshot/restore with local-first merge, covers, version gate |
| `playlist_exchange` | offline | Versioned JSON/CSV playlist exchange roundtrip, preview, atomic export |
| `backup_schedule` | offline | Schedule next-run math, overwrite/keep naming, poll trigger |
| `player_queue` | offline | Pure queue logic: repeat cycles, seeded shuffle distribution, move/remove, edit lock |
| `player_engine` | native | libmpv playback over ffmpeg WAVs: props, pause, af log-verify, gapless EndFile(Eof)->StartFile; SKIP without ffmpeg/libmpv |
| `player_session` | native | Session save/restore with paused state and deferred stream resolution; SKIP without ffmpeg/libmpv |
| `lyrics_providers` | offline | Per-provider fixtures through local mocks: timings, word-level, similarity gate, megalobiz comparison, paxsenix scoring, tidal seam |
| `lyrics_chain` | offline | Order honored, disabled skipped, first-hit wins, miss/hit cached in db, provider errors skipped, tidal plan |
| `mpris_service` | native | MPRIS service on private bus: metadata mapping, action recording, Seeked and PropertiesChanged |
| `notify_actions` | native | Notification show/capabilities/action round-trip against a stub service on a private bus |
| `ui_foundations` | offline | M3E tokens/themes/components via DemoGallery: palette flips, click state, PNG snapshot |
| `sponsorblock` | offline | Segments parse, category filter, active-segment math, skip-once, 404 empty, custom URL |
| `tidal_client` | offline | Device-link login, 0600 token store, search Tracks, HLS manifest, lyrics payload |
| `innertube_home_library` | offline | Fixture home feed maps to models, library/history/writes parse from pinned fixtures, cookie upgrade builds offline |
| `innertube_discover` | offline | Fixture charts/new releases/browse walk/mood deep-link/radio fallback map to models, malformed input errors |
| `shazam_signature` | native | DejaVu signature: determinism, chunked-feed equality, order sensitivity, pinned tone hash, clean wav errors |
| `shazam_client` | offline | amp.shazam.com mock: match parse with links, no-match, 5xx retry, malformed/status errors, rate spacing |

## Visual verification

Flow validated in the spike (`slint-spike/INFORME.md`): Slint 1.19 headless
backend for clicks, virtual clock, drag-reorder, and PNG screenshots.

Harness contract (`madrigal-testkit`):

- Pinned versions: `slint = "=1.19.0"` with the `mcp` feature (without it the
  headless backend does not compile the selector).
- `init_headless()` installs the testing backend with mock clock and software
  rasterizer; call it once per process.
- Clicks: `click` / `click_by_id` (sync, via `mock_single_click`); the canonical
  async pattern is `spawn_local` + `single_click().await`
  (`slint::testing::send_*` does not exist in 1.19).
- Virtual clock: `mock_advance` steps in 1 ms increments because one large
  `mock_elapsed_time` call fires at most 1 tick per timer.
- Drag&drop: the reorder commit goes in `pointer-event(up)`, never in `clicked`
  (click does not fire if the press moved).
- Screenshots: `snapshot_to` uses `Window::take_snapshot()` and saves PNG.
- Qualified `Component::id` ids; the crate compiles the `.slint` with forced
  debug info (`with_debug_info(true)`).
- Crates compiling `.slint` do not inherit `unsafe_code = "forbid"` from the
  workspace: Slint-generated code uses unsafe internally.

