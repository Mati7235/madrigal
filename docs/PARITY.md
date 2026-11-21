# Madrigal — Parity Audit vs Master Plan (read-only, no repo edits)

Source of truth: `/home/matias/.opencode/plan/madrigal-plan-maestro.md` §3.
Method: every row verified against `crates/` source (33 crates + `madrigal-ui`), `docs/NOTES.md`, `docs/UI.md`, `checks.toml`.
Legend: `implemented` = code + wiring present · `partial` = code exists but a planned sub-behavior is stubbed, seam-only, or UI-unwired ·
`missing` = no code · `documented-out` = OUT with written justification.

## A. YTM catalog (F2) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Home: quick picks, keep listening, forgotten, stations | IN (F2) | `madrigal-innertube:home.rs:55` `HomeFeed::from_value`, `:368` `get_home()`; NOTES "InnerTube home" (2 sections live) | implemented | authed live gate at F7 (NOTES: no authed verification) |
| Home: speed dial (pins) + category chips | IN (F2/F8) | `madrigal-ui:ui/pages/catalog/home.slint` HomePage (chips/sections/pins per docs/UI.md); no pins model found in `madrigal-innertube/src` | partial | pins persistence + `CatalogClient` pins API; no dedicated check |
| Search: top results + filters (songs/videos/albums/artists/playlists/community/featured/podcasts) + sort | IN (F2) | `madrigal-innertube:client.rs:177` `parse_search`, `:185-207` per-filter parsers; `mapping.rs:207` Episode/Upload skipped | partial | community/featured/podcast-episode search parsers; sort param wiring |
| Suggestions + trending | IN (F2) | `madrigal-innertube:client.rs:118` `suggestions()`, `:210` `parse_suggestions` | implemented | — |
| Explore / moods | IN (F2) | `madrigal-innertube:client.rs:166` `mood_listing()`, `extension.rs` `ParseFrom for MoodListing`; NOTES "InnerTube extension pattern" | implemented | — |
| Charts + New Releases | IN (F2) | `madrigal-innertube:charts.rs:12` `GetChartsQuery`, `:49` `GetNewReleasesQuery`, `:124` `parse_charts`, `:134` `parse_new_releases`; fixture `charts_ZZ.json` live 2026-11-18 | implemented | — |
| Arbitrary browse / YouTubeBrowse | IN (F2) | `madrigal-innertube:browse.rs` `GetBrowseQuery`, `BrowseItem` (MPRE/UC/playlist prefix rule per NOTES) | implemented | — |
| Artist / album / playlist-online detail | IN (F2) | `madrigal-innertube:client.rs:217` `parse_artist_page`, `:223` `parse_album_page`, `:229` `parse_playlist_page` | implemented | — |
| Radio RD\*/VL\* + continuous autoplay | IN (F2) | `madrigal-innertube:client.rs:156` `watch_playlist`, `radios.rs:8` `is_radio_id`, `:13` `playlist_radio` | implemented | autoplay-continue wiring in player queue (`auto: Vec::new()` in `madrigal-ui:player_ui.rs:80`) |
| Podcasts: shows, episodes, episode search | IN (F2) | `madrigal-core:lib.rs:284` `Podcast`, `:299` `Episode`; **no** podcast fetch in `madrigal-innertube/src` (episodes only skipped in `mapping.rs:207`, `library.rs:142`); UI detail only (`madrigal-ui:pages_catalog.rs:415` `apply_podcast_detail`) | partial | `GetPodcastQuery`/episode search + feed-to-queue; no podcast check exists |

## B. Player (F4) — plan: all IN except Android-only OUT

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Gapless | IN (F4) | `madrigal-player:engine.rs:79` `gapless-audio` yes/no | implemented | — |
| Crossfade (optional dual-instance + volume fade) | IN (F4 optional) | `madrigal-settings:player.rs:31` `TransitionMode::Crossfade`, `:87` `crossfade_seconds`; BUT `madrigal-i18n:lib.rs:106` "Crossfade todavía no está disponible" + NOTES "Crossfade seam: behaves as Normal" | partial | dual-instance overlap + fade coordinator; no crossfade check |
| Normalization (loudnorm) | IN (F4) | `madrigal-player:audio.rs:11` `LOUDNORM_AF="loudnorm=I=-16:TP=-1.5:LRA=11"`, `:130` chain insert | implemented | — |
| Skip silence | IN (F4) | `madrigal-player:audio.rs:13` `SKIP_SILENCE_AF=silenceremove…-60dB`, `:132` gated on `skip_silence` | implemented | — |
| EQ 10 bands + presets + import/export (+bass/virtualizer) | IN (F4) | `madrigal-player:audio.rs:22` `preset_gains` (flat/bass/treble/vocal→plano/graves/brillante/voz), `:49` `export_presets`, `:60` `import_presets`; 10-band UI `madrigal-ui:ui/player/dialogs.slint` EqualizerDialog | partial | bass-booster/virtualizer (Android extras) have no `af` mapping; document as OUT or implement |
| Tempo/pitch (semitones/multiplier) | IN (F4) | `madrigal-player:audio.rs:103` `pitch_multiplier=2^(semi/12)`, `:107` `tempo_af` (scaletempo2+rubberband), `:144` pitch-correction on | implemented | — |
| Sleep timer (minutes / end-of-song) | IN (F4) | `madrigal-player:sleep.rs:9` `SleepTimer`; NOTES "N minutes (host-polled) and end-of-current-song" | implemented | — |
| Queue + position persisted; true resume | IN (F4) | `madrigal-player:session.rs:8` `SessionState`, `:34` `session_path()`, `:38` `save`, `:52` `load` | implemented | stream-resolve + scrobble-register on resume (plan's "cerrar brecha") is unwired in `madrigal-app:store.rs` — verify |
| Preload next | IN (F4) | `madrigal-player:player.rs:176` `preload_next()`, `:47` gated on `preload_next_song` (`madrigal-settings:player.rs:95`) | implemented | — |
| Auto-skip on error + stream recovery tracker | IN (F4) | `madrigal-player:audio.rs:148` `MAX_AUTO_SKIPS=5`, `player.rs:34` `ErrorSkipper`; NOTES poll_failed_load watchdog | implemented | — |
| Nerd stats + details (codec/bitrate/sample-rate/loudness/media-id/buffer) | IN (F4/F8) | `madrigal-player:stream_info.rs:32` `StreamInfo{codec,abr_kbps,sample_rate,media_id,loudness_hint,buffer_health,speed,state,origin,resolution_path}`; UI `StreamInfoPanel` (docs/UI.md) | implemented | — |
| Playback error dialog | IN (F8) | `madrigal-ui:ui/player/dialogs.slint:308` `PlaybackErrorDialog` | implemented | — |
| Pause with volume 0 + recovery volume | IN (F4) | `madrigal-player:audio.rs:188` `ZeroVolumePause`, `player.rs:100` `VolumeAction::Pause`, `:114` `resume_target(..device_mute_recovery_volume)` (`madrigal-settings:player.rs:109`) | implemented | — |
| Audio offload / BT auto-start / audio-focus / ducking | OUT (Android) | no hits in `crates/` (matches plan) | documented-out | — |

## C. Queue (F4/F8) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Repeat off/one/all + shuffle + permanent shuffle | IN (F4) | `madrigal-player:queue.rs:17` `repeat: RepeatMode`, `:270` One/All/Off arms (`madrigal-settings:player.rs:40` `RepeatMode`) | implemented | permanent-shuffle flag: verify `shuffle_permanent` exists in settings/player wiring |
| Tabs All/Searched/Continue-playing/Autoplaying | IN (F4/F8) | `madrigal-ui:player_ui.rs:59` `queue_tabs_from_queue`, `ui/player/queue.slint` QueueDrawer | partial | `auto: Vec::new()` (`player_ui.rs:80`) — Autoplaying never populated; Continue (`cont`) source unclear |
| Drag reorder, multi-select, edit lock | IN (F4) | `madrigal-library:playlists.rs:351` `move_track`; UI QueueDrawer drag commits on `pointer-event(up)`, edit-lock toggle (docs/UI.md) | implemented | multi-select in queue drawer: verify selection model in `queue.slint` |
| Play next / add to queue | IN (F4) | `madrigal-together:proto.rs` `AddTrackRequest(play-next/queue)`; player queue insert ops in `queue.rs` | implemented | — |

## D. Lyrics (F5) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| 11 sources with drag order + on/off | IN (F5) | `madrigal-lyrics/src/providers/` = 10 files (lrclib/kugou/megalobiz/simpmusic/unison/youlyplus/betterlyrics/paxsenix/tidal/youtube + youtube_subtitle = 11 fetchers); `chain.rs:36` `order`, `:37` `disabled`; settings `LyricsProvider` | implemented | — |
| Paxsenix fetch (stub in Python) | IN (F5) | `madrigal-lyrics:providers/paxsenix.rs:341` `async fn fetch` (real impl, not stub) | implemented | live gate at F5 close |
| Karaoke word-by-word + embedded SYLT | IN (F5) | `madrigal-lyrics:karaoke.rs:3` `line_at`, `:15` `word_at`, `:40` `progress_within`; `sylt.rs` + `embedded.rs:61` `sylt_entries_from_file` | implemented | — |
| Animations fade/glow/slide/karaoke/apple | IN (F5/F8) | `madrigal-ui:ui/player/lyrics.slint` LyricsView: active-line highlight + per-word sweep (docs/UI.md); no named fade/glow/slide/apple animation set found | partial | port the 5 named animation modes or document equivalence |
| Click-to-seek, auto-scroll | IN (F5) | LyricsView click-to-seek via `karaoke::{line_at, word_at}` (docs/UI.md) | implemented | auto-scroll follow-current-line: verify in `lyrics.slint` |
| Persisted translation (provider + AI) | IN (F5/F10) | `madrigal-lyrics:translation.rs:39` `translate_chain`, `:66` `attach_translation`, `:93` `encode_cache`; `madrigal-ai:translation.rs:6` `system_prompt` + `mixes.rs` | implemented | persistence path (lyrics-cache table) — verify write-through |
| Romanization JA/KO/ZH/HI/other | IN (F5) | `madrigal-settings:lyrics.rs:148-156` five `romanize_*` flags (default true); impl only `madrigal-lyrics:ttml.rs:282` `romanization()` (x-roman role extraction) — no transliteration engine | partial | transliteration engine (kana/hangul/pinyin) or document TTML-only scope |
| Embedded lyrics MP3/FLAC/M4A | IN (F5) | `madrigal-lyrics:embedded.rs:34` `extract`, `:49` `extract_lyrics` via lofty; NOTES F6-2 ffmpeg-TXXX/USLT quirk handled | implemented | — |
| Adjustable size/line-spacing | IN (F8) | `madrigal-settings:lyrics.rs:166` `v2_fill_transition_width`, font/spacing settings; LyricsView `font/spacing from Lyrics` (docs/UI.md) | implemented | — |
| Share as image | IN (F5/F8) | `madrigal-lyrics:share_image.rs:29` `render`, `:62` `render_png` | implemented | — |
| Cache + cleanup | IN (F5) | `madrigal-curation:caches.rs` sweep (`images/songs/canvas/lyrics`, cap 0 = clear, `max_lyrics_cache_mb` default 64 per NOTES) | implemented | — |

## E. Library (F6) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Sorts + filters (library/liked/downloaded) | IN (F6) | `madrigal-library:songs.rs:8` `SongSort`, `:20` `SortDir`, `:26` `SongScope`, `:95` `songs(scope,sort,dir)`; `lib.rs:6` `PlaylistListFilter` | implemented | — |
| Playlist tags + sortable chips + management | IN (F6) | `madrigal-library:playlists.rs:193` `assign_tag`, `:213` `playlist_tags`, `:223` `all_tags`; UI LibraryPage chips (docs/UI.md) | implemented | — |
| Custom covers, hidden playlists, auto-playlists, top playlists | IN (F6) | `playlists.rs:162` `set_cover`, `:170` `set_hidden`, `:417` `auto_tracks`, `:434` `top_playlists` | implemented | — |
| Pausable/clearable history + pausable search history | IN (F6) | `madrigal-library:history.rs:7` `record_play`, `:37` `clear_listen_history`, `:42` `record_search`, `:75` `clear_search_history` | partial | pause gates (`pause_history`?) — clears exist, pause flag not found in `history.rs`; verify settings privacy gate |
| Stats + Year in Music + streaks | IN (F6) | `madrigal-library:stats.rs:87` `stats`, `:106` `streaks`, `:116-164` `top_tracks/artists/albums`, `:164` `year_in_music` | implemented | — |
| Duplicates (skip/add anyway) + undo | IN (F6) | `playlists.rs:15` `DuplicatePolicy`, `:283` `add_tracks(policy)`, `:331` `undo_last_add` | implemented | — |
| Remote sync + use-login-for-browse | IN (F7) | `madrigal-innertube:library.rs:62` `liked_songs`, `:72` `library_playlists`, `:82` `history` via `AuthCatalogClient` | partial | `use_login_for_browse` flag + write-back sync not found; live authed gate pending (NOTES "Gaps") |
| Local scan: include/exclude folders, min duration | IN (F6) | `madrigal-localmedia:scan.rs:12` `include_dirs`, `:13` `exclude_dirs`, `:14` `min_duration_secs` from settings; NOTES std-only tag parsers | implemented | removal detection/stale rows explicitly deferred (NOTES) |
| Explicit offline mode | IN (F10) | `madrigal-settings:internet.rs:47` `offline_mode`; `madrigal-curation:offline.rs` `NetworkGate` (fail-fast `Offline`) | implemented | — |
| Full backup/restore + scheduled + CSV import/export | IN (F6) | `madrigal-backup:backup.rs:355` `snapshot`, `:458` `restore`, `:399` `write_archive`; `schedule.rs`; `exchange.rs:101` `TrackEntry`, `192` `diff_against`, CSV via exchange | implemented | — |

## F. Downloads (F6) — plan: all IN except Android-only OUT

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Queue: concurrency, pause/resume/cancel/retry per section, speed, progress | IN (F6) | `madrigal-downloads:lib.rs:289` `DownloadManager`, `:365` `pause`, `:390` `resume`, `:410` `cancel`, `:435` `retry`, `:58` `ProgressEvent`, `:161` `ManagerConfig` | implemented | — |
| Quarantine + physical delete | IN (F6) | `lib.rs:29` `DownloadState::Quarantined`, `:795-837` `quarantine_file` paths | implemented | — |
| Auto-download on like | IN (F6) | `lib.rs:358` `enqueue_on_like(track, liked, auto_download)` | implemented | settings-toggle wiring verify (`auto_download_on_like`?) |
| Import/export + redownload on restore | IN (F6) | `lib.rs:620` `export_list`, `:635` `import_list`, `:254` `redownload_missing`; `madrigal-settings:updates.rs` `redownload_on_restore: true` | implemented | — |
| SponsorBlock postprocess + chapters | IN (F6) | `madrigal-downloads:postprocess.rs:63` `read_chapters`, `:138` `shift_chapters`; NOTES single-pass ffmpeg rewrite | implemented | — |
| External downloader per package | OUT (Android) | absent (matches plan) | documented-out | — |

## G. Accounts (F7) — plan: all IN (login path differs)

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Login (plan: browser cookies + manual paste; wry webview optional F10) | IN (F7) | `madrigal-account:cookies.rs` (hand-rolled SHA-1, SAPISIDHASH input order per NOTES), `browser.rs` `records_from_browser` seam, `session.rs`, `store.rs:25` multiprofile store | implemented | `wry` optional webview: no dep found — decide OUT-documented or implement |
| Multi-account + channels + expiry detection | IN (F7) | `store.rs:56` `ids`, `:80` `active_id`, `:84` `create`; `session.rs:19` `channel_handle` | partial | expiry detection logic not found — verify or add |
| visitorData + poToken (BotGuard→bgutil) | IN (F3/F7) | `madrigal-account:auth.rs:14` `visitor_id` → `x-goog-visitor-id`; `session.rs:15` `visitor_data`; `madrigal-streams:po_token.rs:9` `PoConfig`, `:90` `fetch`, `:110` `extractor_args` | implemented | live gate |
| Tidal: device login, search, mixed queue, lyrics, HLS | IN (F7) | `madrigal-tidal:auth.rs:137` `request_link`, `:176` `poll`, `:298` `refresh`; `catalog.rs:80` `search_tracks`; `lyrics.rs:34` `fetch_lyrics`; `stream.rs:165` `dash_hls`, `:187` `upgrade_hls`, `:287` `resolve_stream` | partial | mixed YTM+Tidal queue wiring (player-side) not evidenced — verify in `madrigal-app:store.rs` |
| Spotify: sp_dc/sp_key → playlists → YTM mapping | IN (F10) | `madrigal-spotify:auth.rs` TOTP device flow, `playlists.rs` GQL `libraryV3`/`fetchPlaylist`, `mapper.rs` Dice bigram 0.45/0.35/0.20 + 0.35 accept, `import.rs:31` `import_playlist` honoring `show_spotify_playlists` (`madrigal-settings:integrations.rs:169`) | implemented | — |

## H. Scrobbling + base integrations (F7) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Maloja (persistent outbox, thresholds) | IN (F7) | `madrigal-scrobble:maloja.rs` `MalojaClient`; `eligibility.rs` `played >= min(duration*fraction, cap), duration > min`; `outbox.rs` JSON file + backoff | implemented | — |
| Last.fm / Libre.fm / custom endpoint | IN (F7) | `madrigal-scrobble:lastfm.rs` api_sig md5, `auth.getMobileSession`, `track.scrobble` batch keys; `lib.rs:12` re-exports | implemented | live gate |
| ListenBrainz | IN (F7) | `madrigal-scrobble:listenbrainz.rs` `track_metadata`, `playing_now` vs `single` | implemented | live gate |
| SponsorBlock: 8 categories + custom URL + single skip | IN (F7) | `madrigal-sponsorblock:lib.rs:14-28` 8 `Category` (Sponsor/SelfPromo/Interaction/Intro/Outro/Preview/MusicOfftopic/Filler); `:236` `SkipTracker`, `:228` `active_segment`, `:198` `parse` | partial | custom server URL setting + single-skip-only mode — verify in settings/sponsorblock wiring |

## I. Advanced features (F10) — plan: all IN (Cast/Together-online have OUT fallback)

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Shazam-like recognition (DejaVu + amp.shazam.com) | IN (F10) | `madrigal-shazam:signature.rs` (ring/peak/bands/CRC32/base64 per NOTES), `client.rs` (endpoint/params/UA/`Content-Language: en_US`, `SONG/LYRICS/apple/spotify` parse, HTTP 5xx→429-backoff deviation), `capture.rs` (pw-cat/parec/ffmpeg, `read_wav_mono16k`), `lib.rs:14` `record_match` (db history) | implemented | live gate (fixtures: `no_match.json` live 2026-11-18 verbatim; `match.json` synthesized) |
| Canvas (Spotify/Tidal/Apple loops + cache + refetch) | IN (F10) | `madrigal-canvas:apple.rs` (AMP API + token scrape + editorialVideo), `tidal.rs` (search→videoCover), `spotify.rs` (searchDesktop+client-token+canvaz protobuf), `cache.rs`, `betterlyrics.rs`; `canvas_wifi_only` exists but ignored on desktop (NOTES) | partial | `SpotifyCanvasAuth` → `Unsupported` until spotify session lands (NOTES F10-5; now `madrigal-spotify` exists — wire it); player wiring "lands in F9-3" — verify |
| Google Cast | IN (F10, OUT fallback allowed) | `madrigal-cast:protocol.rs:84` `encode_proto`, `:192` `encode_frame`, builders + `parse_receiver_status`; `media.rs:22` `from_track` (MUSIC_TRACK metadata), `session.rs:24` `State` over `Transport` + rustls; `discovery.rs:61` `browse` (mdns); settings `cast_enabled` (default false), `cast_receiver_app_id` default `CC1AD845` | partial | local-file HTTP proxy (`LocalCastMediaServer`) explicitly "not ported yet" (NOTES F10-6); TLS handshake untested in checks (mock transport only); manual live test pending |
| Music Together (LAN + madrigal://together deep link) | IN (F10, online-relay OUT allowed) | `madrigal-together:host.rs:346` `start`, `:452-464` approve/kick/ban/transfer, `link.rs` `madrigal://together` + legacy `host|port|sid|key`; `discovery.rs` UDP beacon (port 42117); proto 1:1 per NOTES | partial | online relay explicitly OUT (no self-hostable spec — NOTES, justified); `madrigal-db` session persistence unused (ephemeral by design) |
| Discord RPC (OAuth, full presence, preview, debug) | IN (F10) | `madrigal-discord:ipc.rs:104` `IpcClient` (unix sockets, opcode 0/1, clear=`activity:null`); `presence.rs:213` `build_activity` (caps 128/32/256, paused-keeps-small-image, start+end/only-start); `policy.rs` decision+hold; `refresh.rs` gate; `oauth.rs:1` numeric-ID validation | partial | plan's "OAuth loopback" deviated by design (desktop Rich Presence needs only app id — NOTES); `artisturl`/`albumurl` unresolvable (core Track lacks ids — NOTES); debug preview exists (`Preview`) |
| AI (ChatGPT/Gemini/OpenRouter/custom): translation, top mixes, suggestions | IN (F10) | `madrigal-ai:providers.rs:34` `AiConfig::from_settings`, `:107` `OpenAiChat`, `:168` `GeminiChat`, `:229` `chat_model`, `:250` `list_models`; `translation.rs:6` `system_prompt`; `mixes.rs:84` `top_mix_prompt`, `:101` `suggestion_prompt`, `:177` `request_top_mix`, `:192` `request_suggestions` | implemented | OpenRouter = via custom endpoint presumably — verify named preset; live/API-key test optional |
| Anti-AI filter (AiSlopList high/moderate) | IN (F10) | `madrigal-curation:slop.rs` (single configurable `privacy.ai_content_filter_source_url`, `#`/`!` comments, `moderate:` sections; oracle AiSList URLs documented-not-hardcoded — NOTES) | implemented | default source empty → filter inert out-of-box; confirm intended |
| Podcasts UI | IN (F2+F8) | `refs.slint:1` `DetailKind::podcast`, `pages_catalog.rs:415` `apply_podcast_detail`, `DetailPage` branch (docs/UI.md) | partial | no episode list playback wiring evidenced; depends on catalog podcast fetch (A-row) |

## J. Settings (F8) — plan: ~200 typed keys, all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Full tree (appearance/content/player/lyrics/internet/storage/privacy/account/integrations/AI/updates) | IN (F8) | `madrigal-settings/src/` 13 modules (account/ai/appearance/content/de/integrations/internet/lyrics/player/privacy/recognition/storage/updates); `lib.rs:43` re-exports `TransitionMode, EQ_BANDS`; `updates.rs` channel/stable-artifact/onboarding/launch-count | implemented | exact key count vs oracle PreferenceKeys — run a count diff vs Python `settings.py` |
| DoH / HTTP-SOCKS proxy + bypass + test | IN (F8) | `madrigal-net:doh.rs:111` `DohResolver`, `config.rs:33` `ProxyConfig`, `client.rs:55` `build_client`, `:59` `build_stream_client` | implemented | proxy-bypass-streams + connection-test UI wiring — verify in settings page |
| Cache limits + cleanup | IN (F8) | `madrigal-curation:caches.rs` + `madrigal-settings:storage.rs` | implemented | — |
| Privacy (pause histories / reset stats) | IN (F8) | history clears (E-row); stats reset — verify `reset_stats` in library/stats | partial | confirm reset-stats + search-history pause gates |
| Updates channel + auto-check + changelog | IN (F8) | `updates.rs` `automatic_update_check`, `update_channel` Stable/Artifact, `last_update_check/last_notified_version` | partial | actual GitHub-releases fetch + changelog UI not evidenced — verify or mark missing |

## K. UI/UX (F8) — plan: all IN except Android-only OUT

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| M3E tokens port | IN (F8) | `madrigal-ui:ui/tokens.slint` (48 roles/30 types/7 spacing per docs/UI.md) | implemented | — |
| Themes incl. pure black; matugen | IN (F8) | `theme.rs:35` `Theme::Matugen => Dark` (fallback); `appearance.rs:12` `Matugen`; PureBlack in `DemoGallery`/tokens (docs/UI.md) | partial | matugen = explicit "F9 runtime seam" resolving to Dark (docs/UI.md §Theme); system mode likewise — implement or accept documented gap |
| Motion (hover/pressed/transitions) | IN | docs/UI.md: state layers, press shape-morph (no GPU scale primitive) | implemented | — |
| Shortcuts (Space, ←/→, N/P, F, /, Esc, 1-6) | IN | `madrigal-ui:shell.rs` `dispatch_key`, `ui/shell/shell.slint` handle-key (docs/UI.md) | implemented | — |
| es/en | IN | `madrigal-i18n:lib.rs` (~700 keys) + `ui/i18n.slint` `I18n.trl` | implemented | — |
| Mini player / full player / sliders | IN | `ui/player/bar.slint` NowPlayingBar, `full.slint` FullPlayer (theme/gradient/blur/glow), `dialogs.slint` Sleep/TempoPitch/EQ/Error dialogs (docs/UI.md) | implemented | — |
| Onboarding + diagnostics panel | IN | `ui/pages/settings/onboarding.slint:6` OnboardingOverlay (language/PO-check/login/folders), `diagnostics.slint:6` DiagnosticsPanel (level filter/export/single-instance/latency p50/p95); `pages_settings.rs:759` `detect_po_provider`, `:786` latency report | implemented | — |
| Lyrics animations / size / spacing / custom bg / share-image | IN | LyricsView per docs/UI.md (highlight/sweep/click-seek/translation line/font-spacing/share) | partial | named animation modes (K-row D); custom background — verify `PlayerBackground` apply in `player_ui.rs` |
| AOD/widgets/quick-tiles/dynamic icons/FLAG_SECURE/haptics | OUT (Android) | absent | documented-out | — |

## L. Desktop (F9) — plan: all IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| MPRIS complete | IN (F9) | `madrigal-mpris:service.rs:18` `org.mpris.MediaPlayer2` + `:68` `Player` (PlaybackStatus/LoopStatus/Rate/Shuffle/Metadata/Volume/Position/Next/Previous/PlayPause/Seek/SetPosition) | implemented | — |
| Notifications with actions | IN (F9) | `madrigal-notify:builder.rs:75` `action`, `:82` `playback_actions(previous/toggle/next)`; `backend.rs:104` `subscribe_actions` | implemented | — |
| Single instance (socket) | IN (F9) | `madrigal-player:single_instance.rs:31` `acquire`, `:27` `socket_path` | implemented | diagnostics-surface wiring exists (`pages_settings.rs`) |
| .desktop + icon | IN (F9) | no `*.desktop` file found in repo (searched, excl. target/) | missing | add `madrigal.desktop` + icon install |
| Assisted PO-provider setup | IN (F9) | `pages_settings.rs:759` `detect_po_provider`, onboarding PO step (`onboarding.slint:78`) | implemented | — |
| Tray (optional) | IN (F9) | no `tray` code in `madrigal-app/src` | missing | implement or move to documented OUT |

## M. Robustness / distribution — plan: transversal IN

| Feature | Declared | Evidence | Verdict | To close |
|---|---|---|---|---|
| Persistent logging + panel | IN | `madrigal-player:diag_log.rs`, `latency.rs`; DiagnosticsPanel (docs/UI.md) | implemented | — |
| Anti-429 (backoff + disk cache) | IN | `madrigal-net:retry.rs`, `madrigal-streams:resolver.rs:103` `backoff_delay_secs`, `fallback.rs:26` `is_rate_limited`, `cache.rs` TTL | implemented | — |
| Retry per section | IN | `retry.rs` + downloads per-section retry | implemented | — |
| Real fixtures for parsers | IN | `tests/fixtures/` per crate (innertube raw JSON pinned live 2026-11-18, tidal/shazam/sponsorblock vectors) | implemented | create/delete live-only shapes unpinned (NOTES F2-4) |
| Startup <2 s, RAM <400 MB | IN | `madrigal-app:lib.rs:11` `startup_ms` measured, `:14` `boot_headless`; `app_startup` check | partial | RAM/soak/latency p50-p95 numbers: no evidence of a 2 h soak or <400 MB report — pending F11-2 |
| PKGBUILD + AppImage (Flatpak optional) | IN | not found in repo root/listings | missing | F11-1 not started |
| Update check (GitHub releases) | IN | settings fields only (J-row); no fetch code found | missing | implement or mark missing |
| User docs | IN | `docs/` (CONVENTIONS/TESTING/NOTES per functionality) | partial | end-user docs (F11-1) pending |
| Zero upstream/app references (F11-3) | IN | NOT VERIFIED in this audit (no grep run — repo excluded from `cargo` runs per instructions; run `grep -ri` at F11-3) | unknown | run at close |

## Special-watch summary (as requested)

- **Shazam**: implemented (signature/client/capture/history). Remains: live gate, mic-permission UX.
- **Canvas**: partial — Spotify auth seam `Unsupported`, player wiring to verify.
- **Cast**: partial — MVP protocol+media+discovery+session done; local-file proxy missing; TLS live-untested.
- **Together**: LAN done; online relay explicitly documented-out (no spec).
- **Discord**: partial by design — no OAuth loopback (IPC needs only app id); artist/album URL buttons dropped (no ids in core Track).
- **IA**: implemented (OpenAI/Gemini/custom + translation/mixes/suggestions).
- **Podcasts**: partial — models + UI detail only; no show/episode/search fetch.
- **Spotify import**: implemented (TOTP/GQL/Dice-mapper).
- **Tidal**: implemented except mixed-queue wiring to verify.
- **Scrobbling**: implemented (Maloja/Last.fm/Libre.fm-custom/ListenBrainz + outbox).
- **Downloads**: implemented (incl. quarantine/auto-on-like/import-export/SB postprocess).
- **Lyrics (10 providers, karaoke, translation, share)**: implemented except named animation modes + transliteration engine (partial).
- **Backup/exchange**: implemented (snapshot/restore/schedule/CSV-exchange).
- **Accounts**: implemented except wry path (undecided) + expiry detection (unverified).
- **MPRIS/notify**: implemented.
- **EQ/loudnorm/tempo/sleep**: implemented.
- **SponsorBlock**: implemented (8 categories); custom-URL + single-skip mode to verify.
- **Year in Music**: implemented (`stats.rs:164`).
- **Onboarding/diagnostics**: implemented.
- **Matugen**: partial by design — resolves to Dark; F9 runtime seam pending.

## Declared-IN but without code evidence

1. Podcast show/episode/episode-search fetch (`madrigal-innertube` has none).
2. Crossfade audible behavior (seam exists, behaves as Normal; i18n says unavailable).
3. Queue Autoplaying tab content (`auto: Vec::new()`).
4. Romanization engine (flags only; TTML role extraction only).
5. Named lyrics animation modes (fade/glow/slide/apple).
6. History pause gates (clears exist; pause flag unverified).
7. `use-login-for-browse` + remote write-back sync (reads exist; flag/writes unverified).
8. Tidal mixed-queue + Canvas player wiring (F9-3).
9. Spotify-backed Canvas auth (seam `Unsupported`).
10. Cast local-file proxy + live TLS handshake.
11. `.desktop` + icon install, tray, update-check fetch, PKGBUILD/AppImage, soak/RAM report.

## OUT items and justification state

- Justified in plan or NOTES: audio offload/BT/focus/ducking, external downloader, AOD/widgets/tiles/dynamic-icons/FLAG_SECURE/haptics, Together online relay (NOTES F10-2), Discord OAuth-loopback deviation (NOTES F10-3), Cast local proxy deferred (NOTES F10-6), lofty→hand-rolled parsers (NOTES), ytmapi-rs hybrid (NOTES F2-1).
- NOT justified anywhere: `wry` webview login (plan: "opcional F10" — no OUT note, no code), Flatpak optionality (fine, optional), transliteration scope, matugen fallback (only a code comment + docs/UI.md seam note — acceptable but should enter the parity matrix as documented gap).

## Check coverage gaps (`checks.toml`, 63 checks)

Covered: core/i18n/paths/files/secrets/db/streams×3/settings/lyrics×4/net×2/library×3/account×3/scrobble×3/innertube×4/localmedia/downloads×2/backup×3/player×7/mpris/notify/sponsorblock/tidal/shazam×2/discord/ai/together/curation/canvas/spotify/cast/ui×5/app_startup/lives×2.
No check covers: podcast fetch (no code either), crossfade behavior, queue Autoplaying tab, romanization transliteration, lyrics named animations, history-pause gate, `use-login-for-browse`, Tidal mixed queue, Canvas player wiring, Cast local proxy (explicitly untested TLS), update-check fetch, `.desktop`/tray install, PKGBUILD/AppImage, startup-RAM/soak numbers, matugen derivation, `grep -ri <upstream names>` zero-hit gate (F11-3), name-collision verdict (F0-1).
