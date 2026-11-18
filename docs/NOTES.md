# Notes

## InnerTube client decision

Oracle: `upstream_desktop/backends/yt.py` + `backends/playlists.py`,
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
F2-2: wrap YtMusic (auth from browser cookie), search/suggestions/artist/album.
F2-3: playlist read (incl. RD*/VL* fallback), radios, moods, charts-as-custom-query.
F2-4: home-as-custom-query, library/history, playlist writes, rate.
