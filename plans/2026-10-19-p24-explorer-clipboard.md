# P24 · Explorer clipboard

Status: done (owner check on the next alpha pending) · Repo: `<repo>` · Branch: `p24-cN` per atom · Started: 2026-10-19

## Goal

Files copied or cut in Navo paste in Explorer, and files copied or cut in Explorer (or any program that puts files on the Windows clipboard) paste in Navo. Roadmap Alpha item "Explorer clipboard"; owner alpha priority (2026-10-17).

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Keys.** In the file list: Ctrl+C copies, Ctrl+X cuts, Ctrl+V pastes. Commands `files.clipboard_copy`, `files.clipboard_cut`, `files.paste`, bound in `keymap.toml` like every other key, so users can rebind them. They act on the marked items, or on the cursor item when nothing is marked (the F5 rule); the `..` row is never included. Viewer and sheet contexts keep their own Ctrl+C.
- **D2 — Format.** Write what Explorer writes: `CF_HDROP` (a `DROPFILES` block with wide paths, double-NUL terminated) plus the registered `Preferred DropEffect` format (a 4-byte little-endian DWORD). Navo writes 5 (copy | link, as Explorer does) for copy and 2 for move. When reading, a value with the move bit (2) set means move; anything else, or no such format, means copy. ANSI `DROPFILES` (`fWide = 0`) is decoded too, using the system ANSI code page.
- **D3 — Paste.** Paste starts the same copy or move job as F5/F6, into the active pane's folder, without the F5 dialog (like Explorer and Total Commander). Conflicts, progress, pause and undo work as for F5/F6. After a cut is pasted and the move finishes, Navo empties the clipboard if it still holds the same files (Explorer does the same, so a second paste cannot move nothing).
- **D4 — Nothing usable.** No files on the clipboard (text, an image, or virtual files from a zip or phone) shows a status message "Nothing to paste" and changes nothing. Virtual files (`FileGroupDescriptorW`) are out of scope.
- **D5 — Other systems.** Linux is secondary: the clipboard functions return `Unsupported`, and the commands show "The file clipboard works on Windows only". No Wayland/X11 file clipboard in this plan.
- **D6 — Clipboard busy.** Another program can hold the clipboard open. Open it with up to 10 tries, 10 ms apart; then fail with a readable error ("The clipboard is in use by another program") in the status line. Never block the UI thread longer than that.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `navo-shell::clipboard` model | lib | `encode_drop_files(&[PathBuf]) → Vec<u8>`, `decode_drop_files(&[u8]) → Result<Vec<PathBuf>, ClipboardError>`, `Effect::{Copy, Move}` ↔ drop-effect bytes | new | unit tests on every OS |
| `navo-shell::clipboard` platform | lib, Windows FFI | `write_files(&FileClipboard) → Result<(), ClipboardError>`, `read_files() → Result<Option<FileClipboard>, ClipboardError>`, `clear_if_same(&FileClipboard) → Result<bool, ClipboardError>`; Unix: `Unsupported` | new | `tests/clipboard_windows.rs` (Windows CI) |
| file-list clipboard commands | ui | key → selection → shell call → status message or job | new | UI tests with a fake clipboard |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | `navo-shell::clipboard` model and platform (D2, D5, D6) | api-builder (Codex sol xhigh), then reviewer (Opus medium) | xhigh | — | `cargo nextest run -p navo-shell clipboard`; Windows CI runs the integration test | done |
| C2 | Commands, keys, paste job, status messages (D1, D3, D4) | ui-builder (Opus medium) | medium | C1 | UI tests: copy and cut write the marked set with the right effect, paste starts a copy or move into the active folder, cut-paste empties the clipboard, nothing-to-paste message, Linux message | done |

### C1 — clipboard in navo-shell

- **Read:** `crates/navo-shell/src/drives/{mod,windows,unix}.rs` (platform split pattern), `crates/navo-shell/tests/shell_menu_windows.rs` (clipboard use in a Windows test), `crates/navo-shell/Cargo.toml`, `crates/navo-fs/src/code_page.rs` (ANSI code page).
- **Change:** new module `clipboard` with `model.rs` (pure encode/decode, `FileClipboard { paths, effect }`, `ClipboardError { Busy, Unsupported, Malformed, Os(String) }`), `windows.rs` (windows-sys: `OpenClipboard` with D6 retries, `EmptyClipboard`, `SetClipboardData(CF_HDROP)` with a `GlobalAlloc(GMEM_MOVEABLE)` block, `RegisterClipboardFormatW("Preferred DropEffect")`, `GetClipboardData`, `CloseClipboard` always called, also on error) and `unix.rs` (`Unsupported`). Re-export the API from `lib.rs`. Paths keep their exact form; no canonicalisation.
- **Check:** unit tests: round trip of ASCII, non-BMP Unicode, spaces and 300-character paths; ANSI block decode; empty list; truncated or unterminated block is `Malformed`; effect bytes for copy and move and a missing effect. Windows integration test: write two files with Move, read back the same paths and effect, `IsClipboardFormatAvailable(CF_HDROP)` is true; `clear_if_same` empties only when the paths match. Gate passes.
- **Stop if:** the crate's `unsafe` policy or lints forbid the FFI, or a new dependency other than windows-sys features is needed.

### C2 — file-list commands

- **Read:** `crates/navo-ui/src/file_list/keys.rs` (`operate`, `OpKind`), `crates/navo-ui/src/commands/window.rs`, `crates/navo-ui/src/commands/keymap.toml`, how F5/F6 start a job, and the status-line message API.
- **Change:** three actions and commands (D1), injected clipboard service trait so UI tests use a fake; paste starts the F5/F6 job into the active folder without the dialog (D3); after a finished cut-paste call `clear_if_same`; messages for D4, D5, D6.
- **Check:** UI tests listed in the table; keymap and registry snapshots updated; gate passes.
- **Stop if:** the job engine cannot start a copy without the dialog without changing its contract.

## Done when

- On Windows: Ctrl+C in Navo then Ctrl+V in Explorer copies; Ctrl+X then Ctrl+V moves; Ctrl+C in Explorer then Ctrl+V in Navo copies into the active pane. Checked by the owner on the next alpha.
- UI tests and the Windows integration test pass; the gate passes.

## Log

- 2026-10-19 — Plan written from the roadmap and the owner's alpha priorities. Ctrl+C/X/V were unbound in the file list.
- 2026-10-19 — C1 merged (d4c9076): Codex sol xhigh wrote it (a first run stopped because the brief named missing AGENTS.md/CLAUDE.md files); Opus review PASS WITH FIXES, applied by the orchestrator: a failed publish empties the clipboard (no cut that reads as copy), odd padding accepted, clipboard tests in one nextest group with the shell menu tests. Windows compile first happens in CI.
- 2026-10-19 — C2 merged: paste builds the F5/F6 plan with the active folder as target and confirms it at once (`OpRequest.asks = false`); a protected source still shows the typed-DELETE dialog (worker's call, accepted); cut-paste clears the clipboard only after the job is Done; refused pastes say why ("The files are already in this folder", "Cannot paste a folder into itself"). Production adapter in navo-app has no unit tests; the path round trip with Explorer waits for the owner's Windows check. Not done: Explorer-style "- Copy" duplicates when pasting into the same folder.
