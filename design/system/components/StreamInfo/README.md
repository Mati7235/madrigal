# StreamInfo

Technical details of what is playing, from `madrigal-player::stream_info::StreamInfo`.

- Key / value list: source (local or stream), codec, bitrate, sample rate, quality, route (`builtin`, `cache`, `local`), buffer health.
- Keys `body-medium` `on-surface-variant`, values right-aligned with tabular figures, `outline-variant` dividers.
- **Proposal:** buffer health as a small `primary` bar beside its percentage; dialog shape as in Dialog.
- Shows `stream.unknown` / an empty state when nothing is loaded. Slint: `StreamInfoPanel` in `player/dialogs.slint`.
