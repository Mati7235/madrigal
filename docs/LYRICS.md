# Lyrics parsing and providers

Pure parsing foundation (`madrigal-lyrics`) plus the provider chain. No Qt.

## Model

- `Lyrics { lines, synced }`: `synced` mirrors the oracle flag (plain lyrics carry `0` starts with `synced == false`).
- `LyricLine { start_ms, end_ms, text, words, markers }`: `end_ms` is TTML-only (`None` elsewhere); `words` is `None` for plain LRC and `Some` for karaoke input (enhanced LRC, TTML, SYLT).
- `LineMarkers { background, translation, romanization }`: spans the oracle excludes from line text (`x-bg`, `x-translation`, `x-roman`, `ja-latn` transliterations) are preserved here instead of being dropped.
- Times are integer milliseconds; `to_lrc` renders `[mm:ss.xx]` line tags plus `<mm:ss.xx>` word marks.

## Oracle differences (parsing)

- `[offset:]` shifts every timestamp (line and word). The Python oracle discards the tag; here the last tag wins.
- SYLT entries become one line each carrying its single word timing, so `to_lrc` emits word marks. The oracle flattens SYLT to plain LRC through a string intermediate; timings and texts match exactly.
- Entity unescaping covers named (`amp`, `lt`, `gt`, `quot`, `apos`) plus numeric references; unknown entities pass through untouched.

## Translation

- `translation::TranslationProvider` is the chain seam: `translate_chain` tries each provider in order and keeps the first result with a per-line count match. Network providers land with `src/providers/`.
- Timings are discarded like the oracle (`translate_lyrics` translates `[line.text]` only) but the result keeps the source text and attaches the translation per line via `markers.translation`, preserving source `start_ms`/`words`.
- `encode_cache`/`decode_cache` mirror the oracle `{target, lines}` JSON payload; persistence goes through the db `lyrics_cache` table (`cache_lyrics`/`fetch_lyrics`).

## Karaoke

Pure 60fps-consumer helpers (`karaoke.rs`):

- `line_at`: last line with `start_ms <= t`; `None` before the first line.
- `word_at`: last word with `start_ms <= t`, sticky on the last word; `None` before the first word.
- `next_line_start`: smallest `start_ms > t`; `None` when exhausted.
- `progress_within`: 0.0–1.0 sweep of the active word (`end_ms`, else next word start, else done); wordless lines use `end_ms` when present, else a 0.0/1.0 step at `start_ms`.
- A line is active from exactly its start until the next line starts, even past the last word end.

## Embedded

- `embedded::extract` reads tags via `lofty`: USLT/Vorbis `LYRICS`/`MP4 ©lyr` map to `ItemKey::Lyrics` (`lyrics_from_tag`); SYLT is parsed from raw ID3v2 frames because lofty keeps SYLT as opaque binary.
- The ID3v2 scanner supports v2.3/v2.4 frame headers and feeds each SYLT payload to lofty's `SynchronizedTextFrame::parse`, then to `sylt::parse`. Files with unsynchronisation, extended headers, or non-millisecond SYLT timestamps yield no synced entries.
- `extract_lyrics` prefers SYLT; otherwise it parses the unsynced text with `lrc::parse` (plain or LRC-carrying text).

## Share image

- `share_image::render` is glyph-less: vertical gradient from `base`, centered proportional-width line bars, the active line bright and taller, header bars for title/artist. No font pipeline; replace when text rasterization lands.
- `render_png` encodes the buffer with the `image` crate.

## Provider chain

- `providers::Query { title, artists, album, duration_secs, video_id }`; `video_id` is `Some` for YouTube-source tracks only (simpmusic/youtube need it, else they return `None`).
- `chain::plan` follows settings order minus disabled, appends `youtube_subtitle` (when enabled) and `youtube` unless the track is tidal-sourced; paxsenix entries drop out without an API key. Tidal tracks run the `tidal` seam first.
- First `Some` wins; any provider error is a skip. Hits and misses persist through `cache_lyrics`: `v1:lrc\n…` (synced), `v1:txt\n…` (plain/instrumental), `v1:miss` (negative cache). A cached miss or hit performs no HTTP.
- TTML payloads parse through `ttml::parse`, keeping word timings (the oracle renders through an LRC string first and loses words).
- `betterlyrics::extract_ttml` unwraps JSON before the `looks_like_ttml` check; the oracle checks raw text first, so a JSON body containing `<tt` never reaches its unwrapper.
- Megalobiz compares title/artist context around each result link before accepting; the oracle takes the first link unchecked.
- No word-timing reordering: the oracle moves word-level providers first when preferred; the chain honors settings order exactly.
- Seams returning `Unsupported`: `tidal` (native session lands later), `youtube_subtitle` (needs the yt-dlp subtitle pipeline).

## Oracle differences (paxsenix)

- The Android `lyrics` submodule is not initialized in this checkout, so the protocol comes from the Python oracle, which already implements the full flow: `apple-music/search` + `spotify/search` scored by exact/partial title/artist plus duration, then `lyrics/applemusic|spotify` or direct `lyrics/musixmatch`; Apple `content[].timestamp/text[]` items render to LRC, other shapes resolve through the `lyrics/lrc/content/lines/text/plainLyrics/syncedLyrics/line/lyric` keys with `isError/ok/error` rejection.

## Fixtures

All samples are real-derived: the Python repo ships no committed fixture files, so each fixture is copied verbatim (or generated through the oracle) from its check scripts.

| Fixture | Provenance |
| ------- | ---------- |
| `plain.lrc` | `SAMPLE` in `scripts/check_lyrics.py` |
| `enhanced.lrc` | generated by running the oracle `ttml.to_lrc` over the `[3b]` sample in `scripts/check_lyrics.py` |
| `chorus.ttml` | `[3]` sample in `scripts/check_lyrics.py` (x-bg exclusion, `<br/>`, `15.8s` format) |
| `words.ttml` | `[3b]` sample in `scripts/check_lyrics.py` (word spans, nested x-bg span) |
| `sylt.tsv` | `(text, timestamp_ms)` pairs from `tag_id3` in `scripts/check_lyrics_embedded.py`, stored as `ms<TAB>text` rows |
| `embedded_uslt.mp3` | 1s silent MP3 (`ffmpeg anullsrc + libmp3lame 32k`), tagged with mutagen `USLT(eng, "First line\nSecond line")` |
| `embedded_sylt.mp3` | 1s silent MP3 (`ffmpeg anullsrc + libmp3lame 32k`), tagged with mutagen `SYLT(eng, format=MS, [("Hola", 1200), ("Mundo", 3400)])` |

## Provider fixtures

All captures target Yesterday / The Beatles, duration 125 s (`videoId NrgmdOz227I`, lyrics `MPLYt_Cn67yAcHym7-13`), taken 2026-11-18 with spaced live calls.

| Fixture | Provenance |
| ------- | ---------- |
| `lrclib_search.json` | live `lrclib.net/api/search` response, stored whole |
| `kugou_songs.json` / `kugou_candidates.json` / `kugou_download.json` | live kugou song search, lyrics search, base64 LRC download |
| `youlyplus_ttml.json` | live `lyricsplus.prjktla.my.id/v1/ttml/get` response (word-timed TTML) |
| `yt_watch.json` | live `next` response, trimmed to the verbatim `tabbedRenderer` subtree carrying the lyrics browse id |
| `yt_timed.json` | live `browse` (mobile context) response, trimmed to the verbatim `timedLyricsData` subtree |
| `yt_browse_plain.json` | live `browse` (web context) response, stored whole (description-shelf plain lyrics) |
| `simpmusic.json` | derived: oracle `{"data": [{duration, syncedLyrics, plainLyrics}]}` shape; LRC text converted from the captured `yt_timed.json` lines (live simpmusic API now 403s without a SimpMusic client) |
| `unison_video.json` | derived: oracle `{"success", "data": {"lyrics"}}` shape with the repo `words.ttml` body (live search returned `data: []` for both probe tracks) |
| `betterlyrics.ttml` | derived: copy of the repo `words.ttml` as a raw TTML response body (live API now requires an API key) |
| `betterlyrics_wrapped.json` | derived: oracle `{"data": {"ttml"}}` wrap around `words.ttml` |
| `megalobiz_search.html` | derived: oracle `/lrc/maker/download/…` link regex with two results (right + decoy) exercising title/artist comparison (live `/searchall` now 404s after the site redesign) |
| `megalobiz_detail.html` | derived: oracle `id="lrc_…_details"` span holding captured Yesterday LRC lines |
| `paxsenix_search.json` | derived: oracle candidate shape (exact match + decoy with duration) exercising the scorer (needs an API key, not capturable) |
| `paxsenix_apple.json` | derived: oracle `content[].timestamp/text[]` shape with captured Yesterday lines |
| `paxsenix_spotify.json` | derived: oracle `{"lyrics": LRC}` shape with captured Yesterday lines |
| `youlyplus_lrc.json` | derived: oracle `{"lyrics": [{text, time}]}` shape with captured Yesterday lines |

