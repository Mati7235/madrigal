# ConfirmDialog

The shell's two generic overlays: `ConfirmOverlay` and `PromptOverlay` (`overlay` 1 and 2), over a `scrim`.

- Confirm: icon, title, one sentence, Cancel + confirm. A destructive confirm is a filled `error` button; everything else stays `primary`.
- Prompt: title, one TextField (focused on open, label above), Cancel + OK (`prompt.ok`). Enter submits, Esc closes.
- **Proposal:** dialog shape `radius-extra-large` (today 12px) and the scrim at 0.32.
