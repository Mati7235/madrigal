# Madrigal — desktop music player

Plays your local library and YouTube Music (streaming and downloads),
with synced lyrics, scrobbling, and MPRIS control.

## Requirements

- Rust ≥ 1.100 (`rustup toolchain install stable`)
- libmpv (playback)
- yt-dlp (YouTube streams and downloads)
- ffmpeg (conversion and metadata)
- deno or node (PO provider for YouTube Music streaming)

## Checks

`cargo xtask checks` is the only quality gate.

```sh
cargo xtask checks                        # all groups
cargo xtask checks --group offline        # offline | native | live | visual
cargo xtask checks --only queue           # a single check
cargo xtask checks --max-fails 1          # stop at the first failure
```

See `docs/TESTING.md` for the full contract.

## License

GPL-3.0-or-later, see `LICENSE`.
