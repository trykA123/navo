# P22 · A complete, live settings file (D56)

Status: ready · Repo: `<repo>` · Branch: `p22-sN` per atom · Started: 2026-10-17

## Goal

`navo.toml` lists every setting with its value, follows new defaults for keys the user never changed, reloads live when edited by hand, and the settings sheet shows, per row, whether a value is the default or the user's own, with reset and a notice when defaults change.

## Decisions

All decided (D56, L6 decision B); workers never reopen them.

- **D1 — Complete file.** At start, off the UI thread, every missing key is added with its current default in its section, in place with `toml_edit` (comments, order, CRLF/BOM kept), `atomic_write`, one `.bak` per session. Existing values are never changed by this step.
- **D2 — Defaults ledger.** `navo.db` table `settings_defaults(key TEXT PRIMARY KEY, value TEXT, written_at)` records the default Navo wrote. A key is *following* when its file value equals the ledger value; otherwise it is *the user's*. On upgrade, following keys whose default changed are rewritten to the new default and the ledger is updated; the user's keys are never touched. A user who types the default by hand counts as following.
- **D3 — Removed settings.** Keys Navo no longer knows stay in the file with a `# no longer used by Navo` comment above them, added once.
- **D4 — Live reload.** The settings folder watcher (as in P20 K4) reloads `navo.toml` on change; valid → apply live as the sheet does; invalid → keep the last valid settings, show the error with its line in the status line, never rewrite the file until it is valid again. Saves from the sheet must not echo back as a second apply.
- **D5 — Row state.** Each row in the sheet shows Default or Changed; Changed rows have Reset, which writes the current default and marks the key following again.
- **D6 — Upgrade notice.** When following keys change default, a notice lists each (`files.sort: name → date`) with Keep old value, which writes the old value as the user's own.
- **D7 — Fixes from the owner's test (2026-10-17).** The blue TOML key line under a row reserves its space so rows never move; F3 on a key missing from the file goes to its section header, else the last line, with a status note that the default applies.

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | Complete file and defaults ledger, upgrade rewrite, removed-key comments (D1–D3) | orchestrator or Opus high | high | — | store tests: comments and order kept byte-for-byte, user keys untouched, following keys updated, ledger migration | done |
| S2 | Live reload of navo.toml (D4) | Opus medium | medium | S1 | app tests: edit on disk applies, invalid kept and reported, no echo after a sheet save | done |
| S3a | D5 row state and D7 fixes: Default/Changed per row with a changed-from-default dot, Reset on Changed rows (writes the default, marks following), footer "Applied live · Saved to navo.toml · N changed", a "Live" chip; the TOML key line reserves its space; F3 on a missing key goes to its section header or the last line with the status note | ui-builder (Opus medium) | medium | S1, S2, review fixes, P20 K5 | UI tests: state per row after sheet edit and hand edit, Reset, footer count, rows never move, F3 on a missing key; store test for Reset; screenshots both themes | done |
| S3b | D6 upgrade notice with Keep old value (including the late start-up `default_changes` dropped today in `navo-app/src/settings/reload.rs` `receive_late`), and a "Find a setting" field that filters rows by label, key and help | ui-builder (Opus medium) | medium | S3a | UI tests: notice lists each change, Keep old value writes the old value as the user's, a late completion still shows the notice, find filters and clears with Esc; screenshots both themes | done |
| S4 | Review | reviewer (Opus) | medium | all | verdict | blocked |

### S3 brief

- **Read:** this plan (D5–D7), D56, the S1/S2 log lines, `crates/navo-ui/src/settings/` (rows, view, controls), `window_view/settings_news.rs`, `crates/navo-app/src/settings/start.rs` (`default_changes`), `design/prototypes/menus-settings-d43.html` (direction A).
- **Change:** row state Default/Changed from the ledger comparison exposed by navo-store (add a read API if missing; no I/O in navo-ui); Reset writes the current default and marks the key following; "Only changed" filter; the upgrade notice lists `default_changes` with Keep old value per item and Accept all, shown once, never modal; the TOML key line keeps its space so rows never move; F3 on a key missing from the file goes to its section header, else the last line, with a status note; plus the prototype gaps above.
- **Check:** `cargo xtask gate`; UI tests for each behaviour; gallery screenshots light and dark, looked at.
- **Stop if:** the ledger cannot tell Default from Changed for a key without a store change larger than a read API.

## Done when

- A fresh install writes a full `navo.toml`; F3 on any row lands on its line.
- Editing a value by hand applies without a restart; a broken file is reported and the app keeps working.
- A simulated default change updates following keys only, and the notice offers Keep old value.
- The gate and Windows CI are green; the reviewer passes.

## Log

- 2026-10-17 — Plan written after the owner's alpha-17b test (settings sheet praised; rows move when the TOML key appears; F3 opens line 1 for keys not in the file) and the owner's decision that the file holds everything, live, with fine control (D56). Starts after P20 K5 merges, since both touch the settings sheet.
- 2026-10-17 — S1 merged (99e3e6d): `reconcile` = upgrade, complete, mark unused, then a safety check that refuses any result that loses a value; ledger in `navo.db` V3 (`settings_defaults`, canonical TOML values); start-up runs it on a thread before the window, writes only on change through the store's atomic write and session `.bak`, records the ledger after the write; unparsable files are never touched; `default_changes` kept on the start state for S3. 59 new tests incl. a proptest. Keys with no default (`terminal.command`, `editor.path`, `[experimental]`) are not written; a key deleted by hand comes back with the current default.
- 2026-10-17 — S2 merged (40a6b8d): shared `watch_file` for keymap.toml and navo.toml (two watchers on the settings folder); `Reloader` reloads under the store lock, sends Applied or Invalid (file, line, column); echo skipped by comparing with the store's current settings and again in the window; a broken file is never rewritten (save refuses it); deleting the file keeps the current settings; S1's completion runs only at start. No setting needs a restart; the six fields the sheet marks as not applied are unchanged.
- 2026-10-18 — Review fixes branch: three Codex rounds (deleted-file save; eight round-one findings; interrupted-write recovery with a staged `navo.toml.defaults-pending` file) and an Opus review (no lost user value; six small fixes sent to Codex). S3 input: the late start-up completion's `default_changes` are dropped in `navo-app/src/settings/reload.rs` `receive_late`; the upgrade notice (D6) must receive them, or a slow start never shows which defaults changed.
- 2026-10-19 — S3 split into S3a (row state, D7 fixes) and S3b (upgrade notice with late changes, find field); both on Opus medium (owner: medium by default, high only for the hardest).
- 2026-10-19 — S3a merged after review fixes: Default/Changed per row (Changed = differs from the default and from the ledger), dot, Reset button and F8 (no-op on Default rows, hint only on Changed), Ctrl+Z restores, footer "N changed", Live chip, reserved key line, F3 on a missing key lands on its section with a note joined to any load note; the ledger opens once and its errors reach the sheet. Open: late start-up completion leaves no ledger values in memory (S3b); the F-key bar shows F8 blank in settings; p19 screenshots outdated.
- 2026-10-20 — S3b done on `p22-s3b`. Upgrade notice (D6): `SettingsNews::Defaults` carries the start-up and the late `default_changes` (`Loaded.default_changes` via `Started::into_loaded`, `receive_late` announces after its reload); a card at the top of the sheet lists `label` and `key: old -> new` with Keep old value (the existing `change_setting` path, so comments stay) and Dismiss; shown once per change in the session (seen list), plus a calm status-line pointer while the sheet is closed. Find a setting: field above the sheet body (Ctrl+F in the sheet, new `SettingsFind` context), words AND over label, key and help; matches from every section under their headings, empty headings hide, arrows and Tab walk matches only, Down or Enter leaves the field, Esc clears then a second Esc closes, "No setting matches" when empty. Screens `design/screens/p22-s3b-*.png`. Open: a notice not dismissed is gone after a restart (the ledger already moved on); keys with no sheet row are not listed; the late completion still leaves no ledger values in the live store.
