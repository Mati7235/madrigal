# ErrorState

Failures the user can act on: playback error, download failed, offline, search failed. **Intentional addition** next to EmptyState, so errors never look like empty pages.

- **Inline banner** (inside a row, card or page header): `error-container` fill, 16px radius, 20px `error` icon, one `title-small` line, one `body-medium` sentence, and at most one tonal action (Retry).
- **Page error**: centred like EmptyState. A 120px blob in `error-container` holding a 48px `error` icon, `headline-small` title, one sentence that says what failed and what the user can do, and a filled Retry button.
- Never colour alone: the icon shape and the words carry the meaning. Message text is plain: name the thing that failed, skip codes unless the user asked for diagnostics.
- Contrast: `on-error-container` on `error-container` 7.1:1 (Light) / 9.4:1 (Dark) / 9.4:1 (Pure Black).
- Slint: `M3ErrorBanner { text, supporting, action }` in `cards.slint`; `M3EmptyState` takes an `error` flag to switch the blob to `error-container`.
