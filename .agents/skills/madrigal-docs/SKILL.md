---
name: madrigal-docs
description: Write, reground or review Madrigal's documentation in docs/, one document per code block (crate or group of crates), verified against the code at HEAD. Use when documenting how a part of the workspace works, when a doc may have drifted from the code, when moving findings out of NOTES.md, or when a change alters behavior a doc describes. Adapted from documenting-legacy-codebases (riekelt/technical-writer, MIT) to this repo's rules.
---

# Madrigal docs

Adapted from `documenting-legacy-codebases` (riekelt/technical-writer, MIT,
see `LICENSE`). The code has no comments or docstrings by rule, so `docs/` is
the only place that explains how the workspace works and why. A doc tells a
reader who has not opened the code what the block does, how it does it, and
what will bite them.

## Principle

The code at HEAD is the only reliable witness. Names, old docs, `NOTES.md`,
`PARITY.md` and agent reports are testimony: verify in code, then write. A doc
survives a refactor and changes only when behavior changes.

## Docs tree

```text
README.md              repo root: what Madrigal is, requirements, how to run, docs index
docs/
  ARCHITECTURE.md      crate map from Cargo.toml deps and wiring, threads, data flow
  <BLOCK>.md           one per block, see below
  CONVENTIONS.md       existing: workspace layout, naming, commits
  TESTING.md           existing: checks contract
```

Campaign working files live in `.docs-campaign/` (git-ignored), never in
`docs/`. They describe the work, not the system, and `file:line` goes stale on
the next edit:

```text
.docs-campaign/
  findings.md          suspected defects, one line each with file:line
  coverage.md          blocks, status (drafted/reviewed), commands behind counts
  unknowns.md          what was not determined and what was checked first
```

When the campaign closes, none of them is kept: each finding becomes an issue
or a fix (the coordinator asks the user which), each unknown is either
resolved or moved to the block doc's Pitfalls as "not determined: ...", and
the folder is deleted.

Blocks (group crates by what they do together; adjust only on evidence from
the dependency graph):

| Doc | Crates / paths |
|---|---|
| `APP.md` | `madrigal-app` (boot, AppState, workers, timers, UI glue) |
| `PLAYBACK.md` | `madrigal-player`, `madrigal-app/src/playback.rs` |
| `STREAMS.md` | `madrigal-streams` |
| `CATALOG.md` | `madrigal-innertube`, `madrigal-account`, `madrigal-app/src/catalog.rs` |
| `LIBRARY.md` | `madrigal-db`, `madrigal-library`, `madrigal-localmedia`, `madrigal-files` |
| `DOWNLOADS.md` | `madrigal-downloads`, `madrigal-sponsorblock` |
| `LYRICS.md` | `madrigal-lyrics` (existing doc: extend it) |
| `DESKTOP.md` | `madrigal-mpris`, `madrigal-notify`, `madrigal-scrobble`, `madrigal-discord`, `madrigal-app/src/desktop.rs` |
| `SETTINGS.md` | `madrigal-settings`, `madrigal-paths`, `madrigal-secrets`, `madrigal-backup` |
| `UI.md` | `madrigal-ui`, `madrigal-i18n` (existing doc: extend it); `THEMES.md`, `ARTWORK.md`, `A11Y.md` stay as its satellites |
| `INTEGRATIONS.md` | `madrigal-spotify`, `madrigal-tidal`, `madrigal-cast`, `madrigal-together`, `madrigal-shazam`, `madrigal-ai`, `madrigal-canvas`, `madrigal-curation` |
| `NET.md` | `madrigal-net`, `madrigal-core` shared types |

Existing docs are owners: extend them, never start a rival beside them.
`NOTES.md` is dissolved: each finding moves to the doc of the block it belongs
to, then `NOTES.md` gets a one-line pointer banner. `PARITY.md` gets a banner
pointing at `coverage.md` and the block docs; the user decides removal.

## Block doc template

```markdown
# <Block>

Code: `crates/madrigal-x`, `crates/madrigal-app/src/x.rs`

<Two or three sentences: what the block does for the app, conclusion first.>

## How it works
<The mechanism and its consequences. Behavior, not transcription.>

## Threads and state
<Which thread owns what; how results reach the UI thread; what is shared.>

## Flows
<One or two real scenarios end to end, for example "the user clicks a search result and the audio plays".>

## Configuration
<Every settings key and env var the block reads, with default. Dead knobs marked dead.>

## Decisions
<Why it is built this way and what was rejected, dated when it depends on a
third party (e.g. "2026-11-21: only the VISIONOS client streams fully without a PO token").>

## Pitfalls
<Surprising real behavior a contributor will trip over.>

## Checks
<Which checks in checks.toml cover the block, and what is not covered.>
```

Drop a section that has nothing true to say; never pad it.

## Rules

- English, conclusion first. Prose style follows the `unslop` skill (from
  cursor/plugins; vendored in `.agents/skills/unslop`, restored by `skills-lock.json`). Its rules
  apply to every doc; this skill's rules win where they conflict.
- The `Code:` line is the only place with paths. Prose names behavior, not
  functions, types or line numbers. Exception: a public type or settings key a
  reader must search for. `file:line` goes only in `.docs-campaign/findings.md`.
- No delivery history in docs: no phase IDs (F11-3, WP15e), task IDs, SHAs,
  "now uses", "was refactored". Describe the current system.
- Present tense only for code that runs. Wired but disabled code is marked
  dormant. Dead code is mentioned only where a reader would trip on it.
- Alive is proven by wiring (who calls it from `madrigal-app` boot or a UI
  callback); dead is proven by a named search that found no caller.
- Every claim is verified in code or a check. Third-party behavior (YouTube,
  yt-dlp, LRCLIB, MPRIS daemons) carries the date it was observed.
- Defects are noted, not fixed: one line in `.docs-campaign/findings.md`, the doc describes
  the behavior as it is. Docs work never edits code.
- One fact, one home; other docs link to it.
- No comments are added to code to compensate; the doc is the explanation.

## Workflow

1. Survey: for the block, list entry points, UI callbacks, threads, settings
   keys and env vars read, files on disk, external services, checks. Record the
   commands used in `coverage.md`.
2. Read the existing owner doc and any `NOTES.md` sections for the block.
3. Draft from the code, using the template. Mark the doc `drafted` in `coverage.md`.
4. Review by a second agent with fresh context: every claim checked against
   code, prose checked against the rules above. Mark `reviewed` only after that.
5. Unslop pass: load the `unslop` skill and apply it to the finished doc. It
   edits wording only; it never changes a claim, a number or a path.
6. Last: `ARCHITECTURE.md`, the root README docs index, and one end-to-end
   trace across blocks (search, then play, then scrobble) to catch gaps
   between docs.

## Drift check

What keeps the docs true after the campaign is the `Code:` line. When a change
touches code listed there, the same change updates the doc. To find stale
docs, compare the last commit of the doc with the last commit of its code:

```sh
code=$(git log -1 --format=%H -- crates/madrigal-streams)
doc=$(git log -1 --format=%H -- docs/STREAMS.md)
git merge-base --is-ancestor "$doc" "$code" && [ "$doc" != "$code" ] && echo stale
```

A doc reported stale is reread against the code, not assumed wrong.

## Parallel work

One writer per doc; never two agents in the same file. The files in
`.docs-campaign/` are edited only by the coordinator, who merges
the entries each writer reports. Writers are read-only on code.
