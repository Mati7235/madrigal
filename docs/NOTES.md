# Notes

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

