# Notes

## Playlist list data

`madrigal-library::Library::list_playlist_summaries(&PlaylistQuery)`
returns `PlaylistSummary` (`id`, `name`, `subtitle`, `origin`,
`track_count`, `updated_at` in unix ms, `cover`, `capabilities`). Filters:
`text` (case-insensitive name substring), `origin`
(`All`/`Local`/`Account`), `order` (`Recent`/`Name`/`Tracks`), `limit` and
`offset`. Hidden and auto playlists are excluded. `subtitle` is the stored
description; the "N tracks" label stays a presenter/i18n concern.

`track_count` is denormalized and rewritten inside the same transaction as
`add_tracks`, `remove_tracks`, `move_track`, `duplicate_playlist`,
`undo_last_add`, `set_liked` and `madrigal_db::set_playlist_tracks`
(import/restore). `updated_at` (migration v2, backfilled for existing
databases) is bumped by those operations plus rename/description/cover, so
`order = recent` does not depend on playlist rowid.

Account seam: origin `Account` reads locally stored `remote`/`spotify`
playlists only; there is no session or network call. `Local` matches
`kind = 'local'`, so without an account sync the `Account` query is empty.
Merging `AuthCatalogClient::library_playlists` into this query belongs to
the presenter once the app wires `madrigal-account`/`madrigal-innertube`.

## Catalog screens port (F8-3)

Oracle: the local Python app (`ui/qml/{Home,Search,Explore,Detail}Page.qml`)
(port structure/behavior only; no Qt in the product).
`madrigal-ui` hosts `HomePage`/`SearchPage`/`ExplorePage`/`DetailPage`
(`ui/pages/catalog/`) inside `PageHost` branches 0-2; `MainShell` mirrors
every page model as `home-`/`search-`/`explore-`/`detail-` properties and
forwards `chip-selected`/`query-changed`/`suggestion-accepted`/
`filter-selected`/`source-selected`/`play-requested`/`detail-requested`
to the Rust presenter (`src/pages_catalog.rs`, fixture `load_*` helpers).
Slint findings: single-line `TextInput` consumes Up/Down (cursor move) and
Return (`accepted`), so suggestion navigation goes through a new additive
`M3SearchBar::nav-key(string) -> bool` callback wired to the input's
`key-pressed` (accept on handled); the shell's `/` signal reaches the global
top bar through `M3SearchBar::focus-input()` (focus + select-all, additive),
and the bar drives `suggestion-index` while the Search page keeps the rows.
Shell `/`-back handling order: overlays, then `detail-stack`, then sidebar.
Component `changed` handlers were avoided for cross-component state;
read-only bindings and the mirrored in-out props carry it instead.
Element ids repeat inside one `for` loop only (testkit clicks via `nth`).

## InnerTube client decision

Oracle (local Python app): `backends/yt.py` + `backends/playlists.py`,
driven by `application/catalog.py` (ytmusicapi 1.x, Python).

Oracle surface (per ytmusicapi call):
- search: `search(query, filter, limit)` x5 filters (songs/videos/albums/artists/playlists)
- `get_search_suggestions(query)`
- `get_artist(channel_id)`, `get_album(browse_id)`
- `get_watch_playlist(videoId, radio=True, limit=100)` song radio
- `get_playlist(playlist_id, limit)` with RD*/VL* fallback to
  `get_watch_playlist(playlistId, limit=100)`
- `get_home()` first 6-12 sections, item order album/playlist/artist/track
- `get_mood_categories()` + custom `browse(FEmusic_moods_and_genres_category, params)`
- `get_charts(country)`, `get_account_info()`
- library: liked_songs, library_songs, library_playlists, library_subscriptions, history
- writes: rate_song, create/edit/add/remove/move/delete playlist
- auth: browser cookie + on-behalf-of-user header; HTTP 400 retry, HTTP 500 drops user header

ytmapi-rs 0.3.4 coverage (spike-verified live 2026-11-18, unauthenticated):
| Oracle need | ytmapi-rs | Status |
| search x5, suggestions | search_songs/videos/albums/artists/playlists, get_search_suggestions | native |
| get_artist/album, artist_albums | get_artist, get_album, get_artist_albums | native |
| song radio, playlist read | get_watch_playlist_from_video_id (50 tracks live), get_playlist_tracks/details | native |
| moods | get_mood_categories (2 sections live), get_mood_playlists | native |
| library/history, playlist writes, rate, taste, lyrics | matching methods | native |
| get_home (home feed) | no query, no parse type | gap |
| get_charts | no query, no parse type | gap |
| get_account_info, get_liked_songs | no matching method | gap |

VERDICT: hybrid. ytmapi-rs (`simplified-queries`, tokio+reqwest) for all
standard queries; own Query+ParseFrom impls on its `query()`/`json_query()`
client for home, charts, liked songs, account info (F2-2..F2-4).
Rationale: crate is maintained (Nov 2026), MIT, Spike-green, and its
Query trait is designed for extension; a full own client would re-implement
SAPISIDHASH auth and a dozen parsers with no gain.
Risks: crate API churn (0.x); anti-bot breakage hits either option equally;
custom parsers must pin fixtures from live JSON via raw_json_query.

## SponsorBlock + Tidal fixtures (F7-3)

- SponsorBlock segment payload shape (videoID/segments/segment/category/actionType/UUID/votes)
  copied from `RESPUESTA` in `scripts/check_sponsorblock.py`.
- Tidal search item keys (id/title/duration/artists/album/cover), lyrics keys
  (trackId/lyricsProvider/lyrics/subtitles) and playbackinfo keys (trackId/
  audioQuality/manifestMimeType/manifest/bitDepth/sampleRate) follow the field
  names `tidalapi` parses (`media.py`: `Media.parse`, `Lyrics.parse`,
  `Stream.parse`, `StreamManifest.__init__`); values are synthetic.
- Tidal device flow uses the oracle endpoints
  (`/v1/oauth2/device_authorization`, `/v1/oauth2/token` with the
  `device_code` grant, `expired_token` terminal error) but app credentials are
  constructor parameters, never committed.
- MPD fixture is minimal MPEG-DASH carrying what the port reads
  (`mediaPresentationDuration`, `SegmentTemplate` media/timescale,
  `SegmentTimeline` d/r, `Representation` codecs/sampling rate).
- `encryptionType: "NONE"` counts as clear alongside empty, unlike the oracle
  truthiness test; real clear manifests vary between the two.
F2-2: wrap YtMusic (auth from browser cookie), search/suggestions/artist/album.
F2-3: playlist read (incl. RD*/VL* fallback), radios, moods, charts-as-custom-query.
F2-4: home-as-custom-query, library/history, playlist writes, rate.

## Account session port (F7-1)

Oracle (local Python app): `account/{cookies,session,browser_cookies,secret_store,errors,browser_auth}.py`
plus vectors in `scripts/check_account{,_storage}.py`.
Ported to `madrigal-account` minus QtWebEngine login; browser sqlite import
is a `records_from_browser` seam returning Unsupported.

- SAPISIDHASH input order is `{unix_secs} {sapisid} {origin}` (oracle code;
  verified against golden `SAPISIDHASH 1700000000_1a84dc71...` for
  `AbCdEf123` @ `https://music.youtube.com`). Plain `SAPISID` outranks
  `__Secure-3PAPISID`, which outranks `__Secure-1PAPISID`.
- SHA-1 is hand-rolled in `cookies.rs`: adding any crates.io dep forces a
  full workspace re-resolve, and the shared offline index cache is a mix of
  versions (objc2 0.6.4 cached vs 0.6.5 locked) that fails to resolve.
  Keep this crate dependency-free beyond the workspace + serde.
- Cookies file lives in the runtime dir (`profile_cookies_path`), never next
  to the session JSON; both written 0600 via `madrigal-secrets`.
- Fixtures in `crates/madrigal-account/tests/fixtures/` derive from the
  oracle check vectors; every token is fake (e.g. `AbCdEf123`).

## Scrobble backends decision

Oracle (local Python app): `scrobble.py` (Maloja nowplaying/scrobble,
threshold `min(duration*0.5, 180)` gated on `duration > 30`) plus the
Android parity target (`lastfm/LastFM.kt` signed form POSTs,
`ListenBrainzManager.kt` token JSON submits, `ScrobbleManager.kt`
timer with the same threshold shape).

Ported semantics (`madrigal-scrobble`):
- eligibility is float-compared (`played >= min(duration*fraction, cap)`,
  `duration > min`); 30 s tracks never qualify, 31 s needs 16 s played.
- Last.fm api_sig is md5 of sorted key/value concatenation plus secret;
  `auth.getMobileSession`, `track.updateNowPlaying`,
  `track.scrobble` with `artist[0]/track[0]/timestamp[0]` batch keys.
- ListenBrainz payload nests track data under `track_metadata`
  (`playing_now` without `listened_at`, `single` with it).
- outbox is a JSON file under the data dir via atomic writes; transport
  errors and HTTP 5xx/429 retry with doubling backoff, other failures drop.
Transport is `reqwest::blocking` (default TLS, no redirects, per-call timeout);
`http://` and `https://` behave identically and outbox retry semantics are
unchanged (transport errors and HTTP 5xx/429 retry, other failures drop).
No secrets in-repo: Last.fm key/secret/session arrive as call arguments.

## InnerTube extension pattern (F2-2 example)

`madrigal-innertube::extension` parses `GetMoodCategoriesQuery` into the
own `MoodListing` type (adds optional thumbnails over the native output)
via `impl ParseFrom<GetMoodCategoriesQuery> for MoodListing`, fed with
`YtMusic::json_query` plus `into_inner()`. Live (`mood_listing`) and
offline (`parse_mood_listing`) share that impl; `moods.json` pins it.
F2-3/F2-4 follow the same shape for charts/home: own `Query`/`PostQuery`
plus own `ParseFrom`, raw JSON pinned via `raw_json_query`, offline
coverage through `CatalogClient::parse_*` without network.

## Local media scan (F6-2)

Tag parsing is std-only on purpose: no `lofty` in the offline cargo cache,
so mp3 (ID3v2.3/2.4 + MPEG frame walk), flac, ogg vorbis, m4a and wav
parsers are hand-written. Opus-in-ogg returns a clean per-file error.
ffmpeg writes `lyrics=` to a TXXX frame described "USLT", not a real USLT
frame; the mp3 parser accepts both. WAV `fmt ` byte rate sits at body+8,
not body+4 (sample rate). MP4 `©*` atoms are raw 0xA9 bytes, not UTF-8 ©.
`sync_to_db` upserts only; removal detection (stale Local rows whose file
vanished) is a later task and needs mtime/size tombstones first.

## Downloads queue decision

Oracle (local Python app): `downloads/manager.py` (thread pool, yt-dlp
SponsorBlock/ModifyChapters postprocessors, quarantine dir) + `application/downloads.py`.
Port: `DownloadRunner` trait (fake injectable offline; `YtDlpRunner` spawns
yt-dlp with `--newline` parse in production), album/artist/track-number
output paths, db registry via `madrigal_db::upsert_download`.
Findings: `madrigal_db::open` writes (migrations) so registry open+write must
share one mutex or concurrent first-writes hit `database is locked` under WAL;
ffmpeg chapter rewrite must be single-pass (`atrim`/`concat` + `-map_chapters 1`
with re-encode) because `-c copy` + chapters is rejected by the ipod muxer.

## InnerTube home, library and writes (F2-4)

`madrigal-innertube::home` (`GetHomeQuery` on browse `FEmusic_home`,
`ParseFrom` into own `HomeFeed`): sections keep shelf order, items
classified by `pageType` (`MUSIC_PAGE_TYPE_ALBUM/ARTIST/PLAYLIST`,
`watchEndpoint` for tracks); tastebuilder shelf skipped.
`home.json` captured unauthenticated 2026-11-18 via `raw_json_query`
(2 sections: Trending community playlists x10, Quick picks x20);
home varies per request, logged-out included.

`madrigal-innertube::library` (`AuthCatalogClient` over
`YtMusic<BrowserToken>`): liked songs via `GetLibrarySongsQuery`
(`FEmusic_liked_videos` -> `TableListSong`), `library_playlists`,
`history` periods (songs/videos map to `Track`, episodes/uploads
skipped; playlist `owned` stays false, no ownership signal parsed).

`madrigal-innertube::writes`: create/edit/add/remove/move/delete plus
`rate_song` on native methods; edit/add/remove-history pinned from
upstream fixtures, create/delete are live-only (no raw captures exist
upstream for them).

Fixtures from nick42d/youtui `ytmapi-rs/test_json` (real responses,
2024-2025): `get_library_songs_20240701`, `get_library_playlists`,
`get_history_20240713`, `rate_song_like_20240710`,
`edit_playlist_title_20240626`, `add_playlist_items_20240626`,
`remove_history_items_20240704`. No credentials in repo.

Auth seam: `CatalogClient::with_cookies` validates the header with the
`madrigal-account` cookie model (`parse_cookie_header` +
`has_login_cookie`) and assembles the `BrowserToken` offline (field
names pinned to ytmapi-rs =0.3.4); live refresh via
`from_cookie`/`from_str` deferred to the account-phase live gate.
Oracle quirks for the integration (no retry plumbing here, that is
madrigal-net): retry once after 700 ms on HTTP 400; on HTTP 500 with a
user header set, drop the on-behalf-of-user header and retry once.

Gaps: no authed live verification (no user cookies); create/delete
offline shape unpinned; live gate at account phase.

## InnerTube discover surface (F2-3)

New files only (`charts.rs`, `browse.rs`, `radios.rs`); live methods call
the public `raw_json` seam and share the offline `parse_*` code path.
- `GetChartsQuery` posts `browseId FEmusic_charts` with
  `formData.selectedValues`; upstream marks GetCharts "Not Planned"
  (parity table), so no upstream test data exists: `charts_ZZ.json`
  pinned live 2026-11-18 unauthenticated. Chart playlist ids keep the full
  `VL` browseId (browse-consistent, `is_radio_id` true); extra video
  buckets (premium daily/weekly, US genres, IN languages) fold into
  `genres`.
- `GetNewReleasesQuery` posts `browseId FEmusic_explore` and reads the
  `FEmusic_new_releases_albums` / `FEmusic_new_releases_videos` tabs.
  Unauthenticated explore omits premium `top_songs` (`VLPL`); both tabs
  missing is `Err`. Video ids fall back to the item menu when the title
  run carries no play endpoint.
- `GetBrowseQuery` walks grid/carousel/shelf sections into
  `BrowseItem` (prefix rule: `MPRE` album, `UC` artist, else playlist;
  title-run or menu video id means track). Button-only sections
  (moods grid/carousel) are skipped.
- `MoodPlaylistsPage` reuses native `GetMoodPlaylistsQuery` with an own
  `ParseFrom` mapping to core `Playlist`.
- `is_radio_id` (`RD`/`VL` prefix) routes to
  `GetWatchPlaylistQuery::new_from_playlist_id`; `parse_playlist_radio`
  shares the song-radio parse path.

## Shazam recognition port (F10-1)

Oracle (local Android app): `shazamkit` (`ShazamSignatureGenerator.kt` DejaVu port,
`Shazam.kt` amp.shazam.com client, `models/ShazamModels.kt` shapes).

Signature (`madrigal-shazam::signature`): faithful port of the Kotlin
(spreading ring, peak inequalities, band tags `0x60030040+band`, 48-byte
header with CRC32 over `full[8..]`, base64 data URI). Deviations: Hann
window and FFT trig tables use Rust `cos`/`sin`, so last-ulp float drift
vs the JVM is possible; UUID/timezone/geo/user-agent are time-seeded
picks from the Kotlin lists instead of `UUID.randomUUID`/`Random`
(no `rand` dep); steady pure tones yield few or zero peaks by design
(transient suppression), so checks use ffmpeg concat tone sequences.
Generator heap-boxes its FFT tables: the 2 MiB ring literals overflow
the 2 MiB test-thread stack if built on stack before boxing.

Client (`madrigal-shazam::client`): same endpoint path, query params,
User-Agent list, `Content-Language: en_US` and JSON body shape as the
Kotlin `performRecognition`; parsing follows `toRecognitionResult`
exactly (SONG metadata Album/Label/Released, LYRICS text, apple/spotify
providers, video option `v=` or 11-char path tail). Score maps to
`Recognition{score: 1.0}` with the YouTube id as `TrackIdentity`;
extra fields ride in `ShazamMatch.links`. Deviation: HTTP 5xx retries
with the 429 backoff (task requirement; the Kotlin only retries 429).
Transport is reqwest blocking via a direct dep (real HTTPS); proxy/DoH
from `madrigal-net` is a later integration.

Fixtures (`crates/madrigal-shazam/tests/fixtures/`): `no_match.json`
is a verbatim 2026-11-18 live response to a synthetic-tone signature
(HTTP 200 `{"matches":[],"tagid":"<echo>","retryms":9000}`, one call);
`tagid` echoes the request UUID per call. `match.json` is synthesized
from the Kotlin parser field paths only (values invented for tests).

Capture (`madrigal-shazam::capture`): tool order `pw-cat`, `parec`,
`ffmpeg`; all three binaries exist on this machine but no live monitor
is required:
`pw-cat`/`parec` record under `timeout -s INT`, ffmpeg uses `-t`.
`read_wav_mono16k` accepts PCM 8/16/24/32 and float32, any rate or
channel count, resampling linearly to 16 kHz mono; malformed input is
`Error::Wav`, never a panic.

## Discord RPC port (F10-3)

Oracle: `DiscordRPC.kt` (field templates, buttons, timestamps), `DiscordPresencePolicy.kt`
(decision + hold), `DiscordSettings.kt` (config surface + preview), `DiscordPresenceModels.kt`
(type/status enums), `DiscordSocialPresenceClient.kt` (transport).
Transport: Android talks to Discord over the gateway websocket with a user OAuth
token minted by their own backend. Desktop uses the local Rich Presence IPC instead:
unix sockets `discord-ipc-0..9` under `madrigal_paths::runtime_dir()`, frames are
`u32LE opcode + u32LE length + JSON`, opcode 0 handshake `{"v":1,"client_id"}`,
opcode 1 `SET_ACTIVITY` with `pid/activity/nonce`; clearing sends `activity: null`.
OAuth deviation: no user OAuth on desktop, Rich Presence needs only the application
id, stored in `Integrations.discord_client_id` (numeric snowflake, fixtures use
labeled fakes). `DiscordOnlineStatus` has no IPC equivalent and is not sent.
`artisturl`/`albumurl` buttons resolve to nothing: core `Track` carries no artist
channel or album ids. Paused keeps the configured small image with small text
"Paused": no pause-icon URL is invented. Timestamps: playing emits start+end from
`now - position` plus duration; paused emits start only; unknown duration omits
the key. Text caps ported exactly: 128 chars for name/details/state, 32 for button
labels, 256 for URLs, values under 2 chars dropped.

## Music Together port (F10-2)

Oracle (local Android app): `.../together/` package (Ktor WebSocket LAN server+client,
online relay via the author's server infra, `together://` deep links).
Port: `madrigal-together`, LAN-only over std threads + newline-JSON TCP
frames; no tokio (nothing else in the frame path needs it).

Protocol map (message set and state machine ported 1:1, wire differs):
`ClientHello/ServerWelcome/ServerError` handshake with `PROTOCOL_VERSION`,
`RoomState` full-snapshot broadcast, `ControlRequest` (play/pause/seek/
skip-next/previous/seek-index/seek-track/repeat/shuffle),
`AddTrackRequest` (play-next/queue), `QueueMove/QueueRemove` (queue edits
need the add flag), `JoinRequest/JoinDecision/ParticipantJoined/Left`,
`HostTransfer/HostTransferred`, `Kick/Ban`, `ClientLeave`,
`HeartbeatPing/Pong`, `PlayStateSubmit` for guest sync.
Roles host/guest, approve flow (`require_approval`), per-flag gates
(`guests_can_add/guests_can_control`), banned stable `client_id`s,
welcome text in `ServerWelcome`, bounded activity ring
(join/leave/approve/reject/kick/ban/transfer/queue-change/play-state).
Playback converges last-writer-wins on a `version` counter; queue order is
host-authoritative. Deep links: `madrigal://together?host&port&sid&key`
plus `madrigal://together/join?code=` and the legacy `host|port|sid|key`
compact form.
Discovery: UDP beacon (`BEACON_MAGIC`, default port 42117 matching the
oracle pref) with per-host ephemeral ports in checks.

Online mode is an explicit deviation: the oracle relay (`TogetherOnlineApi`,
bearer token, author endpoint) has no self-hostable spec, so `madrigal-together`
ships LAN-only until a self-hosted relay exists. `madrigal-db` likewise
unused: sessions are ephemeral, nothing persists.
Findings: broadcast must never block the read loop (per-client writer
threads + bounded mailbox; welcome is queued before map insert or a
concurrent emit lands first); removal authority is the read loop only
(pump death must not prune, or in-flight submits from vanished peers are
dropped); abrupt drops need graceful `Close` after the notice or kick/ban
frames are lost to RST.

## Content curation (F10-8)

Oracle (Android app): `aicontentfilter/` (`AiContentFilterRepository`,
`AiContentFilterUseCases`), queue `filterExplicit`/`filterVideo`
(`playback/queues/Queue.kt`), `StorageLocationRepository` cache kinds,
`MusicService` song-cache trimmer.
Port: `madrigal-curation` (`slop`, `offline`, `curation::caches`, `content`).
Findings:
- Oracle AI lists are two remote files (AiSList `aislist_blocklist.txt` +
  `aislist_warnlist.txt`) with `!` comments and `normalizeChannelKey`
  (strip `@`, alphanumerics only, lowercase). Madrigal uses one
  configurable source URL (`privacy.ai_content_filter_source_url`, empty
  by default) with `#`/`!` comments, a bare `moderate:` section marker
  and `moderate:<id>` inline prefixes; the AiSList URLs are documented
  here, not hardcoded, so no invented official default ships.
- Oracle `musicVideoType` (OMV/UGC) has no Madrigal `Track` equivalent,
  so `content::flags_of` guesses video-ness from id shape only (bare
  11-char base64url YouTube id; UC/PL/VL/RD/OLAK/MPRE/FE and
  `tidal:`/`local:` prefixes never match). Callers with real metadata
  pass their own `ContentFlags`.
- Cache sweep covers `cache_dir()` subdirs `images/songs/canvas/lyrics`;
  existing writer dirs (`artwork`, `streams`) keep their own prune logic.
  Cap 0 MB clears the dir; `max_lyrics_cache_mb` defaults to 64.
- `NetworkGate` (from `internet.offline_mode`) fails network ops fast
  with `Offline`; local lookups bypass it. No connectivity probe: in
  offline mode the answer is known without touching the network.

## Canvas port (F10-5)

Oracle (local Android app): `canvas/` module (Kotlin).
`madrigal-canvas` ports `CanvasSource` (incl. `from_preference` with
`BOTH` -> `ALL`), identity matching (feat./video qualifiers, primary
artist, NFKD fold), and the per-source fetchers. Player wiring lands in
F9-3; `resolve_playback` returns the playable video URL (mp4 and m3u8
both play in mpv natively). The `canvas_wifi_only` setting exists but is
ignored on desktop (no metered-network detection).

- Tidal: `GET /v1/search` with `types=TRACKS` through the
  `madrigal-tidal` session (sessionId/countryCode/auth header); video
  from `album.videoCover` as
  `resources.tidal.com/videos/{uuid-slashes}/1280x1280.mp4`, still from
  `album.cover`. Fixture is synthetic (field shapes from the oracle
  parser).
- Apple: AMP API `v1/catalog/us/search` (`term/types/limit/10/
  extend=editorialVideo/include=albums`) plus token scrape of
  `music.apple.com` (JWT with `iss=AMPWebPlay`, fallback to
  `/assets/index*.js` scripts) and album lookup for `editorialVideo`.
  Search/album fixtures are real captures (2026-11-18, "Cruel Summer":
  `motionDetailSquare`/`motionDetailTall` hold m3u8 `video` URLs; extra
  keys `motionSquareVideo1x1`/`motionTallVideo3x4` observed but not
  parsed, matching the oracle). Token-page fixture is synthetic (minted
  unsigned JWT, parser checks `iss`/`exp` only, like the oracle). Two
  live calls only (search + album lookup).
- BetterLyrics: `GET artwork.boidu.dev/?s=&a=&storefront=` with the
  oracle 60 s TTL; fixture is synthetic.
- Spotify: full port of the searchDesktop + client-token + canvaz-cache
  protobuf flow runs against injectable bases, but production
  credentials need the spotifycore session (pending F10-4), so the
  `SpotifyCanvasAuth` seam returns `Unsupported` until then
  (`NoSpotifyAuth`; stub auth in the check proves the wiring).

## Audio features port (F4-2)

Oracle (local Python app): `player/audio.py` (EQ bands/presets, loudnorm,
tempo/pitch mapping) driven by `application/playback.py`.
Ported to `madrigal-player` (`audio.rs`, `sleep.rs`):

- EQ bands are the oracle's exact set; presets flat/bass/treble/vocal map to
  the oracle plano/graves/brillante/voz vectors; customs live in
  `equalizer_custom_profiles_json` as `{"name": [gains]}` with JSON
  export/import round-trip.
- `loudnorm=I=-16:TP=-1.5:LRA=11` for Dynamic; replaygain modes map to Off
  (no mpv replaygain wiring in this Engine).
- Tempo is the `speed` property (mpv drives its native scaletempo2, which
  takes no params); pitch semitones map to `2^(semi/12)` via
  `rubberband=pitch-scale`; `audio-pitch-correction` stays on so speed never
  double-shifts pitch.
- Skip silence is `silenceremove` trimming head+tail at -60dB (standard
  values; the oracle has no silence skip).
- Unloadable files emit no EndFile event (mpv 0.42 only logs
  `Failed to recognize file format` and parks playlist-pos at -1), so
  auto-skip pairs the EndFile(Error) arm with a `poll_failed_load` watchdog
  (2 s grace, then playlist-pos -1 advances like EOF); the streak resets on
  FileLoaded and caps at MAX_AUTO_SKIPS.
- Sleep timer modes are N minutes (host-polled) and end-of-current-song
  (fires on the next EndFile(Eof)).

## Crossfade seam

mpv cannot cross-fade across playlist items: one Engine owns one audio chain,
so TransitionMode::Crossfade has no in-process implementation and behaves as
Normal. Plan: dual-instance with a short overlap driven by a fade
coordinator. Volume ramps inside one instance would fake it; not done.

## Spotify import port (F10-4)
`madrigal-spotify`: cookie-based Spotify import (playlists to YTM mapping).
Oracle: `spotifycore` (`Spotify.kt`, `SpotifyAuth.kt`, `SpotifyMapper.kt`,
`models/`) plus app-layer `SpotifyLibraryRepository.kt` (session/expiry rules)
and `SpotifyPlaybackResolver.kt` (resolution rules).

Auth flow: `GET gist nuances.json` (pick max `v` with valid base32 `s`) +
`GET open.spotify.com/api/server-time` feed a TOTP (HMAC-SHA1, 30 s step,
6 digits, implemented dependency-free in `totp.rs`, verified against RFC 6238
vectors) into `GET open.spotify.com/api/token?reason=transport&
productType=web-player&totp=&totpServer=&totpVer=` with
`Cookie: sp_dc=...[; sp_key=...]`; anonymous/blank token means the cookie is
invalid. Session `{sp_dc, sp_key, access_token, expires_at_ms}` lives in
`madrigal-secrets` account `spotify` (0600 fallback); reuse while fresh with
the oracle's 60 s grace, otherwise re-exchange via stored cookies; a 401
refreshes once (rejected-token guard) and only then surfaces `NotLoggedIn`.

Playlists over the partner GQL endpoint (`api-partner.spotify.com/
pathfinder/v2/query`, `app-platform: WebPlayer` + open.spotify.com
origin/referer) with the oracle's pinned persisted-query hashes
(`libraryV3`, `fetchPlaylist`, `profileAttributes`): `libraryV3`
`flatten=true` flat list paginated at 50, missing track totals backfilled via
a `limit=1` `fetchPlaylist` call; `fetchPlaylist` paginated `content.items`
with `itemV2._uri` + `uid`, local tracks skipped at the track level exactly
like the oracle; `PersistedQueryNotFound`/412 surfaces `HashRotated`
(no remote refresh in this crate). Profile reads GQL `profileAttributes`
with the oracle's REST `GET /v1/me` fallback.

Mapper rules ported 1:1: query is `"<first artist> <title>"`; lowercase +
strip `(feat..)`, `(ft..)`, `[..]`, `(remaster..)`, `(remix..)`, non-alnum,
collapse spaces; Dice bigram similarity weighted title 0.45 / artist 0.35 /
duration 0.20 with the oracle's duration ladder (<=2 s: 1.0, <=5: 0.8,
<=10: 0.5, <=30: 0.2, else 0; unknown: 0.5); accept at >= 0.35 with early exit
at >= 0.95. Resolution keeps the YTM title/artists/identity, prefers the
Spotify album name/duration (when > 0)/medium artwork, and stores the
`spotify_id -> Track` mapping in `spotify_mappings` (lazy `CREATE TABLE IF
NOT EXISTS`, no migration bump); misses stay unresolved and never fail the
import. `import_playlist` honors the existing `show_spotify_playlists`
settings toggle (`madrigal-settings` `Integrations`).

Fixtures are synthetic values shaped by the Kotlin parse code (field names it
reads: `uri`/`_uri`/`profile.name`/`albumOfTrack`/`coverArt.sources`/
`duration.totalMilliseconds`/`durationMs`/`is_local`/`content.totalCount`/
`ownerV2`/`pagingInfo`/`accessToken`/`isAnonymous`/`serverTime`/nuance
`s`+`v`); provenance: shapes from `spotifycore` + app repository, values
invented. Mapper test reuses the real `madrigal-innertube` fixture
`search_songs_beatles.json` through `CatalogClient::parse_search_songs`.

## Cast spike (F10-6)

Oracle (local Android app): `.../cast/` on Android uses the GMS Cast SDK
(`media3-cast`) with `DEFAULT_MEDIA_RECEIVER_APPLICATION_ID` (`CC1AD845`,
empty `cast_receiver_application_id` falls back to it), GMS
`MediaMetadata` music-track keys (TITLE/ARTIST/ALBUM_TITLE/images),
volume `0.0..=1.0` coerced, and a local HTTP proxy with Range + CORS so
the receiver can fetch local files.

Crate options researched 2026-11:
- `rust_cast` 0.21.0 (github.com/azasypkin/rust-cast, MIT, sync,
  rustls since 0.19, updated Dec 2025): most downloads, but sync
  blocking API and protobuf dependency; message shapes not asserted
  against the oracle.
- `cast-sender` 0.4.x (github.com/haecker-felix/cast-sender, MIT,
  async, updated Oct 2026): small community, smol runtime, mismatches
  the workspace tokio stack.
- `oxicast` 0.0.5 (tokio, updated Nov 2026): too young (0.0.x).
- `mdns-sd` 0.22.0 (pure Rust, no async runtime): maintained,
  standard choice for `_googlecast._tcp.local` browse.
- Hand-rolled CastV2 framing: the wire format is a 4-byte big-endian
  length prefix around one protobuf `CastMessage`; chosen for the
  protocol layer so CONNECT/LAUNCH/LOAD shapes stay byte-exact with
  the oracle without depending on a third-party protocol crate.

Verdict: implement MVP in `madrigal-cast`. Scope: `protocol`
(hand-rolled protobuf codec + frame codec + message builders and
status parsers), `media` (LOAD payload from `Track`: contentId URL,
normalized contentType, BUFFERED streamType, MUSIC_TRACK metadata
with title/artist/albumName/images), `discovery` (`mdns-sd` browse
plus pure `ServiceRecord` -> `CastDevice` mapping with fn/md/id
fallbacks), `session` (Idle->Connected->Launched->Loaded->Stopped->
Closed over a `Transport` trait; rustls TLS connector with
device-cert acceptance). Settings: `cast_enabled` (default false),
`cast_receiver_app_id` (default `CC1AD845`, sanitized non-empty).
Seams: device-certificate auth challenge is not verified (same trust
model as node-castv2/pychromecast defaults; hardening later);
`check_cast_protocol` covers framing and the state machine over an
in-memory mock transport, not the TLS handshake (no cert generation
in offline checks); the local-file HTTP proxy from the oracle's
`LocalCastMediaServer` is not ported yet. Manual live test: enable
`cast_enabled` on LAN with a Chromecast powered on, browse for the
friendly name, connect (port 8009), launch the default receiver,
LOAD an http(s) stream URL, check audio starts, set volume, stop,
disconnect.


## D-Bus setup timeouts

`org.freedesktop.Notifications` can be registered as activatable (a KDE Plasma
service file) with no daemon behind it; activation then blocks for the bus
timeout instead of failing. `madrigal-app` sets up MPRIS and notifications in
background threads and bounds every call that can hang (`connect`,
`subscribe_actions`, `GetCapabilities`, `Notify`, `MprisService::start`, `sync`)
with `desktop::DBUS_TIMEOUT` (3 s), so boot never waits and shutdown waits at
most one timeout before the feature stays off.

## Provenance

Madrigal is an independent reimplementation of the Android app's behavior and
design; no source code, strings or assets from it are used. Third-party assets:
Lucide icons (ISC, `crates/madrigal-ui/ui/icons/LICENSE`). License: GPL-3.0-or-later.
