# Command center

Status: the box, folders, tabs, recent items, paths and the Search page are in. The disk index is built, not switched on yet.

## What it is

Press Ctrl+K and one box opens. It finds commands, folders and files, like Flow Launcher with Everything. Nothing leaves your machine.

## What you can do

- Run any command by typing a few letters of its name.
- Jump to a recent folder, a folder in an open tab, or any path.
- Find files and folders in the current folder by part of the name. Files anywhere on your disks come with the disk index.
- Run a command that needs a value: Go to folder, Open folder in new tab, Mark by pattern and Unmark by pattern. The box turns into a step with the command name as a chip. Type the folder or the pattern and press Enter. A mistake shows under the input and the box stays open.
- Open the box with nothing typed. It shows your last 5 commands and your 8 most used folders.

The panel is 640 px wide and sits over the title-bar search. It shows up to 12 rows. A click outside closes it. It does not open over a dialog.

Results come in groups: Top hit, Commands (5), Folders (5) and Files (8). "Show all N" rows list the rest.

Prefixes narrow the search:

- `>` shows commands only.
- `/` shows files and folders only.
- A drive letter, `\\` or `~` starts path mode.
- Separate terms with spaces. All terms must match.
- `ext:rs,toml` filters by extension.

Commands follow the focus. Movement and editing primitives are hidden.

## Keys

- Ctrl+K opens the box.
- Up, Down, PgUp and PgDn move the selection. Ctrl+Home and Ctrl+End go to the first and last row.
- Tab and Shift+Tab move between groups. In path mode they complete the folder name.
- Enter runs or opens the row.
- Shift+Enter shows the item in its folder.
- Ctrl+Enter does the same in the other pane.
- Alt+Enter opens it in a new tab.
- Ctrl+Shift+C copies the full path.
- Esc clears the query, then closes the box.
- In a step, Esc clears what you typed. On an empty step, Esc goes back to the search that found the command. After that, Esc works as above.
- Shift+Enter, Ctrl+Enter, Alt+Enter and Ctrl+Shift+C do nothing on a command and in a step.

In a step, Enter on an empty input says what to type. A slow or unreachable folder shows "Checking" under the input; if it does not answer, the box says so and stays where it is.

If Enter comes before the file results arrive, the box waits for the best result and runs that.

A click runs the row. Nothing runs on hover, on a timer or when the selection changes.

## Settings

The `[search]` section has these keys:

- `index`: `auto`, `service`, `walk` or `off`.
- `drives` and `exclude`: which places the index covers.
- `network`: off by default.
- `removable`: off by default.
- `remember`: keep recent use. On by default.

"Clear search history" is a command.

## What was decided

- Fuzzy matching with smart case. Frequently and recently used items rank higher.
- Only commands run from the box count as use.
- No network, no telemetry. Queries are never logged.
- The key keeps its old name internally. The label is now "Command center".
- On NTFS, a Windows service called Navo Index keeps the file list current. You install it with one click and one admin prompt. Only you and administrators can talk to it.
- A Navo Index service from an older version is upgraded to Navo Index. A one-time notice offers an Upgrade button.
- Elsewhere (network drives, non-NTFS, Linux), Navo walks the folders instead.
- Size and date sorting, and the `size:`, `dm:` and `regex:` filters, come after the alpha.
- Targets: commands in under 5 ms, first file results in under 50 ms on 1 million entries, file changes visible within 2 s.

## Done and coming

- Done: Ctrl+K opens the command center in the window.
- Done: recent items, tabs, a live walk of the current folder (it stops at 300,000 entries), and paths.
- Built, not switched on yet: the disk index. It keeps each drive's file ids, so a restart only catches up on changes.
- Built, not switched on yet: the Windows index service. Its Install and Remove buttons are not connected, and Ctrl+K does not search through it yet.
- Coming: the folder-walk fallback.

## Open questions

The plan still lists a few open questions. One covers the Alt+Enter behaviour.

Refs: plans/2026-10-20-p40-command-center.md (D1 to D32)
