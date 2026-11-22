# Packaging

## Supported distros

Debian 13, Ubuntu 24.04, Fedora 42+ and Arch, with glibc 2.39 or newer. Sign-in
requires a Secret Service keyring: GNOME Keyring, KWallet 6 (ksecretd) or
KeePassXC with the Secret Service integration.

## AppImage

```sh
cargo xtask appimage
```

Builds `madrigal-app` in release, stages an AppDir and writes
`$CARGO_TARGET_DIR/appimage/Madrigal-x86_64.AppImage` (or `<repo>/target/...`
without `CARGO_TARGET_DIR`).

| Variable       | Default        | Meaning                                         |
| -------------- | -------------- | ----------------------------------------------- |
| `LINUXDEPLOY`  | `linuxdeploy`  | linuxdeploy binary (AppImage; runs with `APPIMAGE_EXTRACT_AND_RUN=1`) |
| `APPIMAGETOOL` | `appimagetool` | appimagetool binary (AppImage; same)            |
| `YTDLP`        | unset          | standalone yt-dlp to bundle; unset bundles none |

The AppDir contains `usr/bin/madrigal`, `usr/bin/yt-dlp` (when bundled),
`usr/share/applications/madrigal.desktop`, the scalable icon and a custom
`AppRun` that exports `MADRIGAL_YTDLP` only when the user has not set it and the
bundled binary exists.

linuxdeploy bundles libmpv and its dependency tree (ffmpeg, codecs, audio
backends) with its default excludelist: glibc, libgcc, GL/EGL, X11/Wayland,
fontconfig and the kernel interfaces stay host-provided. About 200 libraries.
The keyring backend links `libdbus-1`, which linuxdeploy bundles too. Sources
live in `packaging/`.

## Limits

- glibc baseline is the build host: glibc 2.39 (Ubuntu 24.04). Older distros
  need a build in a container with an older libc.
- linuxdeploy strips binaries; the AppImage ships no debug symbols.
- Running the AppImage needs FUSE; without it use
  `APPIMAGE_EXTRACT_AND_RUN=1`.
- Desktop integration (MIME default handler, menu entry) needs a tool such as
  AppImageLauncher or a manual `desktop-file-install`.
