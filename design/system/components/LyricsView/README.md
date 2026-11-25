# LyricsView

Synced lyrics: the active line follows playback and can be tapped to seek.

- Header: a `Synced` chip (`secondary-container`) or `surface-container-highest` when unsynced, and the −0.1 s / +0.1 s offset buttons around the current offset.
- Lines Inter `title-medium`-size and up in `on-surface-variant`; the active line Figtree, with a per-word sweep in `primary` when word timings exist; translation (`markers.translation`) under it.
- Auto-scroll keeps the active line at one third of the column (300ms ease). A manual flick pauses it for 4s behind the tonal **Back to current line** button.
- **Proposal:** past lines at 0.6 opacity and a larger active line (28/700) so the eye lands on it from across the room.
