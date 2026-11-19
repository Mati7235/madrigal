# Madrigal — Artwork pipeline plan

Read-only investigation. Repo: `/home/matias/Proyectos/Madrigal`.
Oracle (behavior, not code): the Python oracle's `ui/qml/{CoverArt,ArtworkShape,MediaCard}.qml`.

---

## 0. What exists today (facts with citations)

| Surface | Current rendering | Citation |
|---|---|---|
| Now-playing bar | 44×44 circle + first letter text | `crates/madrigal-ui/ui/player/bar.slint:28-46` |
| List rows | 38×38 flat rectangle, no image | `crates/madrigal-ui/ui/components/rows.slint:32-37` |
| Cards | vertical text layout only, no art slot | `crates/madrigal-ui/ui/components/cards.slint:35-54` |
| Full player | 300×300 rect + 96px letter | `crates/madrigal-ui/ui/player/full.slint:153-171` |
| Detail page | header = back button + title/subtitle + description, no art | `crates/madrigal-ui/ui/pages/catalog/detail.slint:39-75` |

Data plumbing gaps (nothing carries art to QML-equivalent props):

- `CatalogItem` struct has `kind/id/title/subtitle/duration/playable` only — `crates/madrigal-ui/ui/pages/catalog/refs.slint:8-15`.
- `item()` builder never receives a thumbnail, and `track_item` drops `track.thumbnail` — `crates/madrigal-ui/src/pages_catalog.rs:19-35`, `:36-51`.
- `LibSongRow`, `LibCardRow`, `DownloadRow` have no art field — `crates/madrigal-ui/ui/pages/library/models.slint:1-7`, `:16-20`, `:33-40`.
- Shell exposes `track-title/artist/album` but no art — `crates/madrigal-ui/ui/shell/shell.slint:53-55`; bar wiring at `:681-698`, full player wiring at `:723-750`.
- `PlaybackUiState` / `apply_playback_state` have no art channel — `crates/madrigal-ui/src/player_ui.rs:92-109`, `:127-145`.
- `madrigal-app` does **not** depend on `madrigal-ui` at all (`crates/madrigal-app/Cargo.toml` deps) and boots headless (`crates/madrigal-app/src/main.rs:28-41`); the UI exists only through `MainShell`/`DemoGallery` and tests.

Slint capability check (1.19.0):

- `Image::from_rgba8(SharedPixelBuffer<Rgba8Pixel>)`, `Image::size()`, `Image::path()` are available unconditionally (`https://docs.rs/slint/1.19.0/slint/struct.Image.html`). No feature gate on `from_rgba8`.
- `Image::load_from_path` / `Image::load_from_data` are gated on crate feature `image-decoders` (same page). That feature is **not** in `slint`'s default list (`std, backend-default, renderer-femtovg, renderer-software, accessibility, compat-1-2, system-tray`, `https://docs.rs/crate/slint/1.19.0/source/Cargo.toml`), and `madrigal-ui` enables only `features = ["mcp"]` (`crates/madrigal-ui/Cargo.toml:24`). Conclusion: **do not use them**; decode with the `image` crate and hand `from_rgba8` a buffer.
- `invoke_from_event_loop(FnOnce + Send + 'static)` is thread-safe and runs on the loop thread (`https://docs.rs/slint/1.19.0/slint/fn.invoke_from_event_loop.html`).
- `SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(raw, w, h)` + `Image::from_rgba8` is the documented integration path (`https://docs.rs/slint/1.19.0/slint/struct.Image.html`, example block).
- `Image` element supports `image-fit: cover` (`https://docs.slint.dev/docs/slint/reference/elements/image/#image-fit`); repo already uses `image-fit: contain` at `crates/madrigal-ui/ui/components/icons.slint:61`.
- `image 0.25.10` and `clru 0.6.3` are already in `Cargo.lock` (`Cargo.lock:2930`, `Cargo.lock:922`), so no new fetch is needed for either.

Existing artwork machinery worth reusing:

- Disk cache primitives already exist: `artwork_dir() = cache_dir()/artwork` (`crates/madrigal-localmedia/src/artwork.rs:26-27`), FNV-1a naming (`:80`), atomic write (`:83`), prune-by-mtime to a byte budget (`:45-75`), `store_artwork(data, mime, limit)` (`:77-87`).
- Budget comes from settings: `storage.max_image_cache_mb` default `512` (`crates/madrigal-settings/src/storage.rs:14`, `:42`), consumed as `artwork_cache_limit_bytes` in `crates/madrigal-localmedia/src/scan.rs:37`, used at `:225-232`.
- Appearance knobs already parsed and surfaced in UI: `hide_player_thumbnail` (`crates/madrigal-settings/src/appearance.rs:199`), `thumbnail_corner_radius` default 12, clamped 0..32 (`:201`, `:267`, `:295-296`), `crop_thumbnail_to_square` (`:203`). Settings UI already lists all three (`crates/madrigal-ui/src/pages_settings.rs:276-278`).
- Embedded art extraction exists for local files (ID3 `APIC`, MP4 `covr`, Vorbis): `crates/madrigal-localmedia/src/tags.rs:14`, `:807`, `:884`; already stored to disk during scan (`crates/madrigal-localmedia/src/scan.rs:225-232`).

Oracle behavior to preserve (not code): 256px sample size, 20px placeholder glyph, `PreserveAspectCrop`, placeholder icon while not ready — `CoverArt.qml:9-10`, `:55-56`, `:61-66`; card art 118px tall inside a 150×190 card, radius 8 (round → width/2), `sourceSize` 300 — `MediaCard.qml:31-32`, `:43-52`, `:54-62`.

---

## 1. Image sources: priority and fallback per case

Source of truth for URLs is already in the models and the DB; nothing new needs to be invented.

| Case | Priority order | Notes |
|---|---|---|
| Remote track (search, home, charts, browse) | 1. `track.thumbnail` (innertube best area, `crates/madrigal-innertube/src/mapping.rs:13-18`, applied at `:35`, `:173`, `:191`, `:202`) 2. album/artist thumbnail of the same page (already denormalized per track at `crates/madrigal-innertube/src/mapping.rs:123-136`) 3. placeholder | URL persisted in `songs.thumbnail` (`crates/madrigal-db/src/lib.rs:24`, upsert `:403-418`, read `:429`) so offline rows still resolve from disk cache |
| Album / artist / playlist card | 1. `Album.thumbnail` / `Artist.thumbnail` / `Playlist.thumbnail` (`crates/madrigal-core/src/lib.rs:231`, `:251`, `:269`) 2. first track thumbnail of the loaded page 3. placeholder | DB columns exist (`crates/madrigal-db/src/lib.rs:34`, `:43`, `:53`, `:90`) |
| Local file (scanner) | 1. sidecar/embedded art stored by scanner (`crates/madrigal-localmedia/src/scan.rs:225-232` → path under `cache/artwork`) 2. `Track.thumbnail` if the file also matched a remote identity 3. placeholder | `artwork_path` is already produced by the scan result (`crates/madrigal-localmedia/src/scan.rs:47`); it is **not** persisted into `songs` (`crates/madrigal-db/src/lib.rs:19-31`) → must be added or re-derived |
| Downloads | 1. `downloads.thumbnail` column (already written, `crates/madrigal-db/src/lib.rs:597-609`) 2. disk cache 3. placeholder | feeds `DownloadRow` which today has no art field (`models.slint:33-40`) |
| Now-playing bar / full player | same as the playing track, but the *already decoded* `Image` is cached by track key so the transition is instant (no re-fetch on carousel) | |
| Now playing, nothing playing | placeholder only (`bar.slint:37` uses `?` today) | |
| Any failure (404, expired URL, decode error) | keep last known good bitmap for that key if any, else placeholder; never blank-flash: never clear `source` before the replacement arrives | |

Remote URL normalization: innertube URLs carry `=wXXX-hXXX` size suffixes (the oracle rewrites them, `CoverArt.qml:32-43`). Normalize to the requested bucket (`=w256-h256`) **before** using the URL as cache key, so 60px and 300px consumers share one entry.

---

## 2. Architecture

One new crate, one UI adapter:

```
crates/madrigal-artwork/          # no Slint, no Qt, no globals
  src/lib.rs        # ArtworkService: request(key, url, size) / cancel / pump
  src/disk.rs       # moved from localmedia::artwork (dir, fnv, store, prune)
  src/fetch.rs      # ThumbSource trait + HttpThumbSource (reqwest blocking)
  src/decode.rs     # image crate: decode -> center-crop -> resize -> RGBA8
  src/mem.rs        # LRU of decoded bitmaps (bytes budget)

crates/madrigal-ui/src/artwork.rs # Slint adapter: Image::from_rgba8 + invoke_from_event_loop
crates/madrigal-ui/ui/components/cover.slint
```

Rationale: business logic stays Slint-free (repo rule mirrored from the upstream app), and the crate is unit-testable with an injected `ThumbSource` (no network, no event loop).

### 2.1 Ownership of the disk cache

Move `crates/madrigal-localmedia/src/artwork.rs` into `crates/madrigal-artwork/src/disk.rs` verbatim (same FNV, same layout, same `cache_dir()/artwork`), then re-export from localmedia (`crates/madrigal-localmedia/src/lib.rs:1-5`) and point `crates/madrigal-localmedia/src/scan.rs:6,226-231` at it. Two dirs for the same thing would be a bug; `madrigal-artwork` must be a leaf dep (`madrigal-paths`, `madrigal-files`, `image`, `clru`, `reqwest`, `serde`), so `localmedia` and `ui` can both use it without cycles.

### 2.2 Memory cache

- Key: `(normalized_url_hash, requested_px)`.
- Value: `Arc<Rgba { w, h, pixels: Vec<u8> }>` (already resized, ready for `from_rgba8`).
- Budget: byte-based, default **24 MB**, hard cap from a new setting `appearance.artwork_memory_mb` (or reuse `storage.max_image_cache_mb` for disk and add a separate small key). Reference math: 256²×4 B = 256 KB → 24 MB ≈ 96 covers; 300²×4 B = 360 KB.
- Eviction: LRU via `clru` (already in lock). Pin the currently-playing track's entry (`pinned: Option<Key>`) so the full player never evicts its own art during a scroll.

### 2.3 Disk cache

- Dir: existing `cache_dir()/artwork` (`crates/madrigal-localmedia/src/artwork.rs:26-27`), so local scanner art and remote art live together and one prune covers both.
- Name: `fnv1a64(normalized_url) . <ext>`; keep content-hash naming for bytes coming from tags as today (`:80`).
- Written through `madrigal_files::atomic_write_bytes` (`crates/madrigal-files/src/lib.rs:40`), as today (`artwork.rs:83`).
- TTL: none. Entries are content-addressed by URL+size, so a stale file is either overwritten by prune or unreferenced. Expiry of signed URLs is handled by "miss → refetch", not by TTL.
- Cleanup: `prune_artwork` on startup **and** every 32 writes (today every write, `artwork.rs:85` → O(n) `read_dir` per image; unacceptable when a grid requests 40 thumbs at once).
- Budget: `storage.max_image_cache_mb` × 1 MB (`crates/madrigal-settings/src/storage.rs:14,42`); also expose the current size in the existing diagnostics hook.

### 2.4 Fetch

- `reqwest` **blocking**, not async: every crate in the repo uses blocking (`crates/madrigal-net/Cargo.toml:10`, `madrigal-ai:16`, `madrigal-scrobble:16`, ...), there is no tokio runtime in `madrigal-ui` (`crates/madrigal-ui/Cargo.toml`), and blocking keeps the call site simple.
- Worker pool: 2 dedicated `std::thread`s + `mpsc` channel + `Condvar`, LIFO stack so the most recent visible items win (a carousel emits requests faster than 2 threads can drain).
- Dedupe: in-flight `HashSet<Key>`; requesting the same key twice enqueues once.
- Coalesce by key, not by slot; cancellation is "drop the result" (see generation guard).
- Timeout 8 s, one retry, `Accept: image/*`, cap response body at 4 MB, reject `content-type` that is not an image.
- Behind a trait so tests inject a fake:
  ```rust
  pub trait ThumbSource: Send + Sync { fn fetch(&self, url: &str) -> Result<Vec<u8>, Error>; }
  ```

### 2.5 Decode / resize

- `image` crate 0.25 (`Cargo.lock:2930`), `ImageReader::new(Cursor).with_guessed_format().decode()`.
- Center-crop to square when `appearance.crop_thumbnail_to_square` (`crates/madrigal-settings/src/appearance.rs:203`), otherwise preserve aspect and let `image-fit: cover` crop in the shader/software renderer.
- Resize to `ceil(logical_px * scale_factor)` capped at 512 (`Window::scale_factor()`), Lanczos3. Requested sizes: rows 38→96, bar 44→128, cards 176→384, full player 300→640. Bucket to powers of two (96/128/384/640) so cache keys stay few.
- Alpha: Slint `from_rgba8` expects straight (non-premultiplied) RGBA; use `into_rgba8()` from the `image` crate, which is straight alpha.
- Decoding happens on the worker threads, never on the UI thread.

### 2.6 Delivery to Slint

```rust
// madrigal-ui/src/artwork.rs — sketch
let img = slint::Image::from_rgba8(
    slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba.pixels, rgba.w, rgba.h),
);
let weak = shell.as_weak();
let gen = slot_generation.load(Ordering::Relaxed);
slint::invoke_from_event_loop(move || {
    if let Some(shell) = weak.upgrade() {
        if gen == shell.get_cover_generation() {   // stale result guard
            shell.set_cover(img);                  // Image is Send+Sync
        }
    }
})?;
```

- `invoke_from_event_loop` is the only place touching the component (`https://docs.rs/slint/1.19.0/slint/fn.invoke_from_event_loop.html`); never `set_*` from the worker thread.
- Generation guard: each `set_now_playing(key)` / row-model refresh bumps a `u64` per slot. A late result for an old key is dropped → fixes the carousel race (track changes while a 300px art is in flight).
- `Image` is cheap to clone and `Send + Sync`, so the same bitmap can be assigned to bar + full player + detail without another decode.

### 2.7 Why not `Image::load_from_path` on the disk cache

It would need `features = ["image-decoders"]` (`crates/madrigal-ui/Cargo.toml:24`), it decodes on the UI thread inside the renderer, it cannot resize, and it cannot be unit-tested for "did the right byte range reach the component". `from_rgba8` with our own `image` resize gives deterministic sizes, DPI control, and the LRU actually holds *decoded* pixels.

---

## 3. UI API

### 3.1 `crates/madrigal-ui/ui/components/cover.slint`

```slint
export component Cover inherits Rectangle {
    in property<image> source;
    in property<length> art-size: 48px;
    in property<length> art-radius: Shape.medium;
    in property<bool> round: false;
    in property<color> placeholder-bg: transparent;
    in property<color> placeholder-fg: transparent;
    in property<string> fallback: "";      // single letter when no art
    in property<length> glyph-size: 20px;

    width: root.art-size;
    height: root.art-size;
    border-radius: root.round ? root.art-size / 2 : root.art-radius;
    clip: true;
    background: root.placeholder-bg;

    // placeholder painted underneath: an empty Image draws nothing, so no
    // "is-empty" boolean is needed in Slint
    Text { text: root.fallback; ... }
    img := Image {
        width: 100%; height: 100%;
        image-fit: cover;
        source: root.source;
    }
}
```

Design choices:

- No `has-art` property: an empty `image` paints nothing, so the placeholder text sits below and is covered the moment the bitmap lands (oracle does the same via `visible: status !== Image.Ready`, `CoverArt.qml:61-66`).
- `art-size` + `round` map one-to-one onto today's literal sizes, so call sites become one-liners.
- Radius comes from `appearance.thumbnail_corner_radius` (default 12, `appearance.rs:267`) and `hide_player_thumbnail` (`:199`) is honored by the shells, not by `Cover` itself.
- Accessibility: `accessible-role: image` with the track title passed as `accessible-label`.

### 3.2 Call sites

| File | Change | Sizes |
|---|---|---|
| `crates/madrigal-ui/ui/player/bar.slint:28-46` | replace `bar-thumb` content with `Cover { art-size: 44px; round: true; ... }`; keep the `Text` fallback inside `Cover`, delete `bar-initial` | 44 |
| `crates/madrigal-ui/ui/player/full.slint:153-171` | `Cover { art-size: 300px; art-radius: bg-style == 2 ? 150px : 24px }`; keep the existing `animate border-radius` on the wrapper | 300 |
| `crates/madrigal-ui/ui/components/rows.slint:32-37` | `art` rectangle → `Cover { art-size: 38px; art-radius: Shape.small }`; add `in property<image> art` | 38 |
| `crates/madrigal-ui/ui/components/cards.slint:35-54` | add `in property<image> art` + `in property<bool> show-art`; when set, art strip on top (`height: 118px`, radius `Shape.small`), texts below; card grows 120 → ≥ 200 (`crates/madrigal-ui/ui/pages/catalog/section.slint:61-63` currently forces 200×120) | ~176 |
| `crates/madrigal-ui/ui/pages/catalog/detail.slint:39-67` | insert `Cover { art-size: 112px; art-radius: 12px }` at the left of `detail-top` (before the back button) or below it for narrow widths; add `in property<image> art` | 112 |
| `crates/madrigal-ui/ui/shell/shell.slint:53-55` | add `in-out property<image> cover`; forward to bar (`:681`) and full player (`:723-750`) | — |

Contexts (all of them) need the same `art` prop forwarded: `section.slint:58-79` (grid) and `:27-49` (rows), `home.slint:92`, `explore.slint:72,104,117`, `library.slint:396,420`, `detail.slint:108,137`.

---

## 4. Tasks (ordered, with exact files and acceptance)

### T1 — Extract the disk cache into `crates/madrigal-artwork`
Files: new `crates/madrigal-artwork/Cargo.toml`, `src/lib.rs`, `src/disk.rs` (move of `crates/madrigal-localmedia/src/artwork.rs`), `crates/madrigal-localmedia/Cargo.toml`, `crates/madrigal-localmedia/src/lib.rs:1-5`, `crates/madrigal-localmedia/src/scan.rs:6,226-231`.
Acceptance: `cargo test --test check_localmedia_scan` still passes; `madrigal_localmedia::{artwork_dir, store_artwork, prune_artwork, cache_size_bytes}` keep working (re-export); the temp dir from `check_localmedia_scan` still contains exactly one file per unique embedded image (existing behavior, `scan.rs:225-236`).
Extra: change `store_artwork`'s prune to every 32nd write (`artwork.rs:85`) — keep a counter in a `Mutex<u32>`; log nothing.

### T2 — Fetch + decode + memory LRU, headless-testable
Files: `crates/madrigal-artwork/src/{fetch.rs,decode.rs,mem.rs}`, workspace `Cargo.toml` untouched (members glob), new check `crates/madrigal-artwork/tests/checks/check_artwork_cache.rs`, register in `checks.toml` and `[[test]]` in `crates/madrigal-artwork/Cargo.toml`.
API: `ArtworkService::new(Arc<dyn ThumbSource>, ArtworkConfig)`, `fn request(&self, key: ArtKey, url: String, px: u32)`, `fn pump(&self) -> Vec<(ArtKey, Arc<Rgba>)>`, `fn cached(&self, key) -> Option<Arc<Rgba>>`.
Acceptance:
- fake `ThumbSource` returns a 64×64 PNG generated in-test with `image`; after `pump()` the bitmap is 64×64 and byte-identical to a direct decode.
- second request for the same key performs **zero** fetches (assert on a counter) and returns the same `Arc`.
- `px: 32` request returns 32×32 (resize path).
- corrupt bytes → `Err`, no panic, nothing written to `cache/artwork`.
- `prune_artwork` keeps the dir under budget after 40 stores with `limit = 3 * file_size`.

### T3 — Slint adapter in `madrigal-ui`
Files: new `crates/madrigal-ui/src/artwork.rs`, export in `crates/madrigal-ui/src/lib.rs`, `crates/madrigal-ui/Cargo.toml` (dep `madrigal-artwork`; dev-dep `image`), `crates/madrigal-ui/src/shell.rs` stays untouched.
API: `pub fn attach_cover(shell: &MainShell, service: &Arc<ArtworkService>)`, `pub fn set_now_playing_art(shell: &MainShell, key: &str, source: ArtSource)`.
Acceptance:
- setting an art with a fake source eventually leaves `shell.get_cover()` with `size() == (128, 128)` after `kit::mock_advance` / a loop of `slint::run_event_loop_until_quit`-free pumping: use `kit::mock_advance(Duration::from_millis(50))` (pattern: `crates/madrigal-ui/tests/checks/check_ui_foundations.rs:78`).
- race test: emit key A then key B before pumping; only B's bitmap is applied (generation guard).
- no direct `set_*` from the worker thread — grep-level assertion: `src/artwork.rs` contains exactly one `invoke_from_event_loop` call site.

### T4 — `Cover` component + demo + headless check with embedded PNG
Files: new `crates/madrigal-ui/ui/components/cover.slint`, `crates/madrigal-ui/ui/gallery.slint` (new `cover-demo` block with `objectName: "cover-demo"` and a `demo-cover: image` property), `crates/madrigal-ui/src/lib.rs` (demo wiring), new check `crates/madrigal-ui/tests/checks/check_ui_artwork.rs`, `checks.toml` + `crates/madrigal-ui/Cargo.toml` `[[test]]`.
Test content:
1. build a 32×32 checkerboard PNG in memory with `image` (dev-dep), feed it through `ArtworkService` → `Image`.
2. `assert!(gallery.get_demo_cover().size().width == 32)` — fails today because the property does not exist; passes only when the image actually arrives.
3. `kit::snapshot_to(gallery.window(), &tmp.join("cover.png"))` after `gallery.show()` (`crates/madrigal-ui/tests/checks/check_ui_foundations.rs:88-89`) and assert the PNG is not a single flat color (guards "Image renders but is invisible/clipped").
4. `Cover` with empty source renders the placeholder letter: snapshot differs from (3).
Registration requirement: `cargo xtask checks --only ui_artwork` resolves `check_ui_artwork` and runs it; a missing `[[test]]` silently becomes `SKIP` (`xtask/src/lib.rs:190-199`), so both entries are mandatory.

### T5 — Thread art through the models
Files: `crates/madrigal-ui/ui/pages/catalog/refs.slint:8-15` (+ add `thumbnail: string`), `crates/madrigal-ui/src/pages_catalog.rs:19-35,36-51,53-70` and every `item(...)` caller (`:80-140`), `crates/madrigal-ui/ui/pages/library/models.slint:1-7,16-20,33-40`, `crates/madrigal-ui/src/pages_library.rs:138-200,430-450`, `crates/madrigal-ui/ui/shell/shell.slint:53-55`, `crates/madrigal-ui/src/player_ui.rs:92-109,127-145`.
Acceptance: `check_ui_catalog` and `check_ui_library` fixtures carry non-empty thumbnails (`crates/madrigal-core/src/lib.rs:200` already has the field) and the existing assertions still pass; `PlaybackUiState.art` is optional so all current tests compile unchanged; adding a thumbnail to a fixture surfaces it in `CatalogItem.thumbnail` (one new assertion per check).

### T6 — Wire the five surfaces + settings
Files: `bar.slint`, `full.slint`, `rows.slint`, `cards.slint`, `detail.slint`, `section.slint`, `home.slint`, `explore.slint`, `library.slint`, `pagehost.slint:312-330`, `shell.slint:681-750`, plus `crates/madrigal-ui/src/artwork.rs` consumer helpers.
Acceptance: `cargo xtask checks --group offline` green; `check_ui_shell`, `check_ui_player`, `check_ui_catalog`, `check_ui_library` snapshots change only where art was added; `hide_player_thumbnail = true` yields a bar without art (new assertion in `check_ui_artwork`); `thumbnail_corner_radius = 0` yields square corners (assert on the `Cover` `border-radius` via a `get_` property or a snapshot diff).

---

## 5. Risks

1. **Memory.** The oracle process idles around 600 MB; the Slint side is a separate budget and a naive cache of decoded 640px art at 40 visible cards = 40 × 640²×4 B ≈ 65 MB, plus 24 MB LRU, plus retries. Mitigation: bucket sizes (96/128/384/640), byte-budgeted LRU with `clru`, pin only the playing track, never cache the full-size original bytes after decode. Add a check that asserts `mem.len_bytes() <= budget` after 200 inserts.
2. **Decoding on the UI thread.** `Image::load_from_path` would decode inside the renderer and also requires the `image-decoders` feature that is not enabled (`crates/madrigal-ui/Cargo.toml:24`). Any accidental direct decode (e.g. calling `decode()` from a `set_` handler) freezes the loop for 10–40 ms per JPEG. Mitigation: decode only in `madrigal-artwork` workers; the UI adapter's only job is `from_rgba8` + `invoke_from_event_loop`.
3. **Carousel / stale results.** Next-track spams 3 requests (bar 128, full 640, detail 384). A late 640 result can overwrite the new track's art. Mitigation: per-slot generation counter + drop-if-stale in the event-loop closure; dedupe in-flight by key; LIFO queue so the newest request is served first.
4. **Expiring / rate-limited URLs.** Innertube thumbnails are unsigned today, but signed variants (`lh3.googleusercontent.com/...=s...`) exist and will 403 after expiry; a URL-keyed disk cache then looks "valid" but stale. Mitigation: on decode/fetch failure, delete the disk entry and retry once with a fresh URL from the model (`Track.thumbnail` is refreshed on every browse); never block the row on failure. Also cap concurrency at 2 to avoid ytimg throttling during a 40-card grid.
5. **DPI.** A single 44px bitmap rendered on a 2× display is visibly blurry; requesting physical size means the same logical cover needs different cache entries per monitor. Mitigation: bucket by `scale_factor` (1×, 2×) in the key, cap at 512px, and re-request on `Window::scale_factor` change only for the playing track.
6. **Two disk caches.** `cache/artwork` is already written by the scanner with content-hash names (`crates/madrigal-localmedia/src/artwork.rs:77-87`) while remote art wants URL-hash names; both live in the same dir and one prune (`:45-75`) deletes by mtime. If local art outlives its play count it will be pruned and the scanner does not re-run automatically (`crates/madrigal-localmedia/src/scan.rs:225-236`). Mitigation: on a local-media cache miss, re-extract embedded art from `Track.local_path` (`crates/madrigal-localmedia/src/tags.rs:14`) instead of showing a placeholder.

---

## 6. Open decisions (need sign-off)

- New setting `appearance.artwork_memory_mb` vs. hardcoding 24 MB. *(Recommend: hardcode for v1, no settings churn; the disk budget already exists at `crates/madrigal-settings/src/storage.rs:14`.)*
- Whether `Lyrics`/queue rows also get art (`crates/madrigal-ui/ui/player/queue.slint`) — out of scope here, but `QueueRowData` has no id today (`crates/madrigal-ui/src/player_ui.rs:54-59`), so it would need a new field.
- Whether artwork rendering should wait for `madrigal-app` to actually link `madrigal-ui` (today it does not: `crates/madrigal-app/Cargo.toml`), i.e. whether T3 is verifiable only through tests until the shell is wired to the binary.
