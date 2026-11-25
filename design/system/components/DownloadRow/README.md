# DownloadRow

A 54px download row with every state the downloads crate reports.

- Columns: 36px cover, title / kind, state (110px), 120×4 progress, speed (72px, only while downloading), actions (92px).
- States: Queued (indeterminate bar), Downloading (determinate + speed, pause and cancel), Paused (`outline` fill, resume), Done (`primary`, check icon), Error and Cancelled (`error`, warning icon, retry and remove).
- **Proposal:** a state icon beside the word, so done and error differ by shape as well as colour; the indeterminate bar uses `easing-emphasized`.
- Below 700px of page width the progress and speed columns hide.
- Slint: `DownloadsPage` in `pages/library/downloads.slint`.
