# Known bugs

Open bugs found by the owner or by reviews that are parked on purpose. Each line: date, what happens, how to reproduce, where it was seen. Fixed bugs move to the log of the plan that fixed them.

## Open

- **2026-10-19 · Flaky Windows CI test `shell_menu_windows::a_loading_submenu_fills_later`.** On run 39197766470 the runner's "Send to" submenu stayed Loading for 15 s; the rerun passed. The test depends on the runner's shell state; consider a longer deadline or skipping it when Send to never fills on the runner.
- **2026-10-17 · The app menu always says "Installed"** (`navo-app/src/app_menu/mod.rs:19`) while the shipped `.exe` is portable (D3); portable mode (D10) is not built. Fixed by P19 L7b (word removed until portable mode exists).
- **2026-10-17 · Settings rows marked "Not applied yet": font, font size, light and dark theme names, terminal provider and command.** They are saved but not used anywhere. Needs a plan (theme loader D16, font settings, terminal providers).
- **2026-10-17 · A long Recycle pauses only when it ends** (P19 L4b log). Pause and Resume work for copy and move; Recycle Bin batches are one shell call.
- **2026-10-17 · The pause tests do not re-check the part-file size** (P19 L4b review, nice-to-have).
- **2026-10-17 · GPUI's own log output is not bridged into the diagnostics log** (P19 L6 open item).
- **2026-10-16 · Follow shows no new lines (owner, alpha-2026-10-16c, Windows).** Open a log in F3, press Alt+F, append lines from VS Code and save: nothing new appears. Parked by the owner ("il notam ca bug si il rezolvam mai tarziu"). Next step: a checker reproduces it on Windows CI with a writer process (append in place, and a VS Code style save) and proves the root cause before any fix.
- **2026-10-16 · Enter in an archive shows "A folder: its files are listed by their full path." on a file row (owner, alpha-2026-10-16b, an `.xlsx`).** Not reproduced on Linux with an xlsx-like zip or with directory entries; regression tests added. Waiting for the owner's exact steps (row, filter text).
- **2026-10-16 · Opening a text file builds the editor on the UI thread** (about 48 ms per MB in release; 0.6 s for 15 MB). Found while fixing the Backspace freeze.
