# Changelog

Each entry is a pre-release build for Windows. Downloads are on the GitHub releases page.

## alpha-2026-10-21d — 2026-10-21

### New

- Ctrl+K finds files anywhere through a disk index, not only in the open folders. By default it indexes your user folder.
- Settings › Search now applies the index mode (Auto, Service, Walk, Off), the drives, the skipped names, and network and removable drives.
- While the index builds, Ctrl+K and the activity strip show "Indexing C: · 12,345 files".
- Settings › Search shows whether the Windows index service runs, with Install and Remove. Windows asks for admin rights; cancelling changes nothing.
- The index service keeps a whole-disk index of NTFS drives, so a new file shows up in a second. It is optional; without it Ctrl+K indexes the folders itself.
- F4 on a folder opens the folder in your editor, as Total Commander does.
- F9 copies the full path of the file under the cursor, or of every marked file, one per line.

### Fixed

- Ctrl+K: a dead network share blocks only itself; local drives answer at once.
- Index service: only your account can ask it, an untrusted install folder is replaced instead of reused, and a failed reinstall puts the old version and its settings back.
- Index service: a client that stops reading or sends garbage is dropped without stopping the service.
- Index service: a damaged or oversized saved index is rejected and rebuilt; a saved index with deleted folders still loads.
- Index service: searches keep answering while the disk is busy.

### Known problems

- Dragging files from Ferrite into other apps (VS Code, Explorer) does not work yet. It comes next.

## alpha-2026-10-21c — 2026-10-21

### New

- Ctrl+K: Enter opens the result.
- Ctrl+K: Shift+Enter shows the result in its folder.
- Ctrl+K: Ctrl+Enter opens the result in the other pane.
- Ctrl+K: Alt+Enter opens the result in a new tab.
- Ctrl+K: Ctrl+Shift+C copies the path of the result.
- Ctrl+K: Alt+Enter does nothing on a command.
- Ctrl+K: "Go to folder…", "Open folder in new tab…", "Mark by pattern…" and "Unmark by pattern…" ask for their value in the box.
- Ctrl+K: Tab completes folder names in a value step.
- Ctrl+K: in a value step, Esc clears the text first.
- Ctrl+K: an empty value shows "Type a folder" or "Type a pattern".
- Ctrl+K: a slow path shows "Checking…".
- Multi-rename: Ctrl+Z puts a whole batch back.
- Multi-rename: the preview highlights what changes and what Find matched.
- Multi-rename: a path over 260 characters shows a warning.
- Rich ELF view: the flash map gives each group its own colour. Its files use shades of that colour.
- Rich ELF view: every tile shows its size and share. Tile areas are true to size.
- Rich ELF view: the groups that do not fit go into one "N more" tile.
- Rich ELF view: shares based on estimated sizes show "~".
- Rich ELF and MAP view: reserved space has its own hatched part.
- Rich ELF and MAP view: a region with unknown free space shows a thin bar.
- Rich ELF and MAP view: the inspector shows exact bytes.
- Rich ELF and MAP view: the address strip and the map have keys.
- Rich ELF and MAP view: Alt+G regroups and keeps zoom and position.
- Rich ELF and MAP view: empty strips take no Tab stop.
- Rich ELF and MAP view: Esc clears a pick before the view closes.
- MAP view: the status line names the parts that were not read.
- MAP view: the Size column hides when the map has no sizes.
- TASKING .mdf files open as a table of chips, spaces and addresses.
- Settings has a new Search page.
- The Search page sets the file index mode, the drives, the skipped names, and network and removable drives.
- The Search page also sets whether searches are remembered. It clears the search history.
- Settings rows that the index does not use yet say "Not applied yet".

### Fixed

- Ctrl+K: Enter typed quickly runs the best result.
- Ctrl+K: the old dialog entries for marking by pattern no longer show twice.
- Ctrl+K: a dead network share no longer piles up background work or blocks local folders.
- Rich ELF and MAP view: bars never show 100% while space is free. Tiny values show "<1%".
- Rich ELF and MAP view: keys and focus keep working after a rebuild.

### Built but not switched on yet

- Windows index service: it keeps a whole-disk index. It is built but not switched on yet.
- The Search page's Install and Remove buttons are not connected yet.
- Ctrl+K does not search through the index yet.

## alpha-2026-10-21b — 2026-10-21

### New

- Multi-rename opens in its own window. Press Ctrl+M, or press F2 with several items marked.
- Multi-rename also opens from Rename in the context menu, and from Ctrl+K.
- Build new names with masks and tokens. Press Ctrl+Space to see the tokens.
- Multi-rename finds and replaces text, changes case and adds a counter.
- The preview shows old and new names. A clash shows "Already exists", also for hidden files.
- Press Del to leave a row out of the rename.
- Enter renames exactly what the preview shows. If the folder changed, Ferrite checks again first.
- Ctrl+S saves a set of rules as a preset. Ctrl+O opens the preset list.
- Multi-rename remembers the last pattern you used.
- Rich ELF view: in Rich mode (View menu, Alt+V), memory regions show their usage, an address strip and a treemap of flash use.
- The Rich ELF view shows the symbol table beside the map. Pick a block or a group to name it in the inspector and the status line.
- Linker fill shows as hatched areas. Simple mode looks as before.
- Rich MAP view: the same screen for linker MAP files. It shows reserved space, groups by module, object file or section, and a table of located sections.
- A MAP file next to its ELF file takes section kinds from the ELF file.
- Ctrl+K finds files and folders in the active pane's folder as you type. It stops at 300,000 entries and says so.
- Ctrl+K path mode: type a path to list its folders. "Go to" appears when the folder exists. Tab completes the name.
- A network folder that does not answer shows a note. Typing never waits for it.
- An empty Ctrl+K box lists your last 5 commands, then your most used folders. A place found in several ways shows once.
- Ctrl+K command rows show where a command applies, such as File list or Viewer. They show the key bound there.

### Fixed

- Ctrl+K: Enter pressed right after typing runs the result for what you typed. Commands show at once, and file results follow.
- Ctrl+K: highlights mark the right letters in names with combined accents or emoji.
- Ctrl+K: characters such as ! and ^ match as typed.
- Ctrl+K: with remember turned off in settings, no history is used or shown.
- Multi-rename: the [ ] token buttons look the same in both themes.

### Built but not switched on yet

- Multi-rename: the undo journal keeps each batch, also after a restart. Ctrl+Z to undo a batch comes next.
- Drag and drop with Explorer: Ferrite can take files dropped from Explorer. It can drag files out to Explorer. Explorer never deletes what Ferrite moves. The file list does not use this yet.
- File index: each drive's index saves its file ids. After a restart it only catches up on changes. The index service comes next.

## alpha-2026-10-21a — 2026-10-21

### New

- Drag and drop onto tabs: a drop goes into that tab's folder.
- Drag and drop onto path crumbs: a drop goes into that folder.
- Drag and drop onto a drive pill: a drop goes into the root of that drive. The drive must be connected.
- Drops onto these targets show the same badge and refusals as drops onto folder rows. Esc cancels.
- Alt-drop creates shortcuts. On Windows a shortcut is named "name - Shortcut.lnk". Ctrl+Z removes them.
- Alt-drop into the same folder is refused for now.
- Ctrl+K remembers the folders you visit and the files you open, and it finds them.
- Ctrl+K also finds the folders open in your tabs.
- With an empty box, Ctrl+K lists your 8 most used folders.
- Settings file, [search]: remember = false turns off all remembering, also for commands.
- ELF view: noise symbols that the compiler adds are hidden. The status line counts them. Alt+H shows them.

### Fixed

- Drag and drop: if the list changes during a drag, Ferrite refuses the drop. It says "The list changed during the drag. Nothing was moved." Before, a drop could move a different file or go into another folder.
- Drag and drop: on Windows, the badge treats upper and lower case as the same, as the copy does.
- Ctrl+K: key hints no longer sit under the scrollbar. Hints show Ctrl+V and Alt+Left.
- A security update to a network library.

### Built but not switched on yet

- Multi-rename: the pattern, find and replace and case fields, and the preview table with its summary are in. The window that holds them comes next.
- Rich ELF and MAP view: used flash now counts initialised data copies. TASKING ROM copies count once, under their module. A chip mapped twice gets numbered names. The screens come next.
- File index: on Windows, Ferrite reads NTFS volumes and their change journal directly. This needs admin. A folder keeps its contents when it is renamed between two journal reads. It is not connected to Ctrl+K yet.

## alpha-2026-10-20r — 2026-10-20

### Built but not switched on yet

- Multi-rename: a batch runs as one job, and one Ctrl+Z puts every name back, also after a crash. If putting names back stops halfway, the batch stays pending and is retried; nothing is overwritten. The dialog comes next.
- Rich ELF and MAP view: usage bars per memory region with used and free, a colour legend, a file-bytes bar, and the lookup of the matching MAP or memory definition next to the open file. The screens come next.
- Ctrl+K file index: a folder index that loads its saved copy at once, rescans in the background and follows changes. It is not connected to Ctrl+K yet.

## alpha-2026-10-20q — 2026-10-20

### New

- Drag and drop between the panes: drag a file, or the marked files, by its icon or name onto the other pane or onto a folder row.
- A drag moves by default on the same drive and copies across drives. Hold Ctrl to copy. Hold Shift to move.
- A badge next to the pointer says what will happen. Drops into the same folder or into itself are refused with a reason. Esc cancels.
- Settings, Files: "Drag and drop" asks Never (default), Only moves or Always before a dragged move runs.
- Ctrl+K learns. Commands you run from it often move up the list.

### Built but not switched on yet

- Alt-drop shortcuts: the badge shows "Create shortcuts". Shortcuts can be made and undone, also on folders with very long paths. A drop does not create them yet.
- Rich ELF and MAP view: memory colours and the View menu parts (usage bars, who uses) are in. The screens come next.
- Multi-rename: the engine, Ctrl+M and the keys are in. The dialog comes next.
- Settings file: a new Search section for the file index (what to index, drives, folders to skip). The index itself comes later.

## alpha-2026-10-20p — 2026-10-20

### New

- Activity strip: a line at the bottom of the window shows running work. It has Pause, Show and Dismiss.
- Alt+J opens a drawer with every activity.
- The job card stays hidden. Ctrl+J shows it, and it appears when a job fails.
- Ctrl+K opens the command center in the window. The app menu and the title-bar search field also open it.
- The command center finds commands only. Files and folders come later.

### Fixed

- The recycle chip said "Delete". It now says "Recycle".

### Built but not switched on yet

- Multi-rename: a rename that fails halfway can always be put back.
- Multi-rename: audio tags (artist, album, title, track, year) can be used as name fields.
- Drag and drop: a drag badge.
- Drag and drop: an "Ask before a dragged move" setting in Settings, Files.

## alpha-2026-10-20o — 2026-10-20

### Fixed

- Search by path works with Windows path separators.

### Built but not switched on yet

- Ctrl+K file index: a name table, parallel search, a saved index file and a folder walker.

## alpha-2026-10-20n — 2026-10-20

### New

- MAP view: the Memory, Sections and Symbols tables have resizable columns. Drag a column header, or double-click it to fit.
- MAP view: long names keep their module or prefix.

### Fixed

- Moving files within one drive shows correct progress again.

### Built but not switched on yet

- Memory map model: regions, owners, groups, a strip and a treemap. Linker fill is counted apart.
- Multi-rename engine: Total Commander masks, clean-up, file and photo fields, and safe ordering.

## alpha-2026-10-20m — 2026-10-20

### New

- ELF symbols table: drag a column header to resize it. Double-click fits a column to its content.
- ELF symbols table: long names keep their prefix.
- HEX and S-record views show 32 bytes per row on wide windows.
- Padding gaps under 256 bytes fold into the rows and show as `--`, so a range reads as one block.

### Built but not switched on yet

- Groundwork for Ctrl+K: a command list, ranking and a file index table.

## alpha-2026-10-20l — 2026-10-20

### New

- PDF outline loads faster on long documents. It says so when it cannot be read.
- The current chapter in the PDF outline follows the page you are on.

### Fixed

- Safer PDF links: a link opens only when it is a plain http, https or mailto address. Links with hidden or shell characters are refused. This replaces the link handling of the previous build, which could pass odd links to Explorer.
- Tab in the Go to page field no longer moves between links.

### Built but not switched on yet

- TASKING `.mdf` memory regions and initialised RAM count as data in the memory model. The Rich view comes later.

## alpha-2026-10-20k — 2026-10-20

### New

- PDF links: a click goes to the linked page. A web link opens in the browser. Only http, https and mailto links open.
- A back stack remembers where you were in a PDF.
- The F-key bar is off by default. Settings > Display > F-key bar turns it on.
- F1 lists every shortcut.

## alpha-2026-10-20j — 2026-10-20

### New

- Multi-select: mark files with the keys, with Shift or Ctrl click, with a right-drag range, or with a left-drag lasso from empty space.
- Ctrl plus lasso toggles the marks in the area.
- Marks survive the quick filter. The / key brings back saved marks.
- The footer counts marked files that the filter hides.
- File icons for PDF, Office, audio, video, code and automotive files.

### Fixed

- TASKING maps: part headers and the Space column are fixed.

### Built but not switched on yet

- Groundwork for drag and drop and PDF links.

## alpha-2026-10-20i — 2026-10-20

### Built but not switched on yet

- DBC and A2L description files are read by the automotive models. The viewer comes later.

## alpha-2026-10-20h — 2026-10-20

### New

- New syntax colour themes: Oxidised iron (the default) and Mocha. Choose them in Settings, Display, Text.
- Vector BLF CAN logs open in the CAN trace view.

### Fixed

- TASKING linker maps with long, wrapped names parse cleanly.
- A newer session file is never overwritten by an older Ferrite.
- The database is checked after a crash.

## alpha-2026-10-20g — 2026-10-20

### New

- Alt+M opens the context menu for the marked files. With nothing marked, it opens for the file under the cursor. Menu and Shift+F10 still work.
- PEAK TRC CAN traces (versions 1.1, 1.3, 2.0 and 2.1) open in the CAN trace view. The label shows the format.

## alpha-2026-10-20f — 2026-10-20

### New

- The Markdown preview split and the folder preview sheet parts are in the View menu (Alt+V).
- A setting now replaces the old split memory for each file type.

## alpha-2026-10-20e — 2026-10-20

### New

- Ctrl+F in Settings filters the sections by name, key and help text.
- Up, Down, Tab and F3 move through the matches. Esc clears the search. A second Esc closes Settings.
- If no setting matches, Settings shows "No setting matches".
- After an update changes the default of a setting you never set, a calm card lists each change. For example: the file sort order, from name to date.
- "Keep old value" keeps the old value as yours. Ctrl+Z undoes it. Dismiss closes the card.
- While Settings is closed, the status line points to the card.
- "Keep old value" only writes while the setting still has the new default. It never overwrites a value you chose in the meantime.

## alpha-2026-10-20d — 2026-10-20

### New

- The View menu hides or shows parts of the main window. Open it with Alt+V in the file list, or with the View button in the title bar.
- The View menu can hide the tabs row (when it has one tab), the path bar, the drive button, the search box, the F-key bar, file icons, the Ext, Size and Date columns, and each footer part.
- Changes save at once. Reset to defaults restores the parts. The same options are in Settings, Display, Window.
- Hidden parts keep their keys. Ctrl+L, the F-keys and Ctrl+F3 to Ctrl+F6 still work.
- Sorting by a hidden column shows it in the header, for example "Name by Size ↓".
- Errors, the job pill and the crash notice are never hidden.
- The text viewer has an encoding picker. Use the header button, Ctrl+Shift+E, or Alt+V and then Encoding.
- The encoding picker offers Auto or one of 20 encodings. The choice applies to that file only.
- A decode that loses data opens read-only, with a note.
- Big UTF-16 files, and big Windows-125x and ISO-8859 files, open as paged text instead of Hex.
- Big Shift_JIS, GBK, Big5 and EUC files open as Hex, with a note.
- Files with mixed line endings can be edited. Each line keeps its own ending on save. Unedited lines never change.
- If a change is too large to keep every ending, nothing is saved. A note says so.

## alpha-2026-10-20c — 2026-10-20

### New

- Automotive: TASKING linker maps (`.map` files with TASKING content) open in a map view on Alt+5 in the F3 viewer. Other `.map` files stay text.
- Three tables: Memory, Sections and Symbols. Switch between them with Alt+T or a click.
- Memory shows region, length, used, free and used %. Sections shows address, size, region and space. Symbols shows address, size and space.
- Ctrl+F3 to Ctrl+F6, or a header click, sorts the table. Typing filters by name. Esc clears the filter.
- Ctrl+B opens the navigator: regions with used and size, then object files, largest first. Enter shows only that region or object.
- Ctrl+I opens the inspector. It shows region totals and top objects, a section's range and contributions, and a symbol's section and object.
- Ctrl+G goes to a hex address and selects the symbol that holds it.
- The status line shows toolchain, counts, used and free space. Problems come first, such as unreadable rows or inconsistent regions.
- The map is read in the background, with progress.

### Fixed

- Viewers shrink properly in narrow windows.

## alpha-2026-10-20b — 2026-10-20

### New

- Copy mode setting in Settings, Copy. Auto is the default. It copies several files at once where the drives allow. Standard copies one file at a time.
- The F5 and F6 dialog shows which mode the job uses.

### Fixed

- Auto copy now runs in parallel in the app. It copies up to 4 files from SSD to SSD, and up to 8 to or from a network drive.
- A running job keeps the mode it started with.
- Move (F6) onto an existing file with Overwrite now works on the same drive. The old file stays aside until the job ends. Undo, or recovery after a crash, brings both files back.
- If another program takes the name meanwhile, the old file goes next to it as "name (before copy).ext".

## alpha-2026-10-19h — 2026-10-19

### New

- Automotive: ELF files open in a symbol view on Alt+5 in the F3 viewer. Files ending in `.elf`, `.axf` or `.out` count, and so does any file with an ELF header. An `.out` file that is not ELF stays text.
- The symbol table shows name, address, size, type, bind and section. C++ and Rust names show demangled. Plain C names show as they are.
- Typing filters the symbols by name. Every letter and digit goes to the filter. Esc clears it.
- Ctrl+F3 to Ctrl+F6, or a header click, sorts by name, section, address or size.
- Ctrl+G goes to a hex address. It selects the symbol that holds it, and shows its source file and line.
- Ctrl+B opens the navigator: header, sections and load segments. Enter shows that part's symbols.
- Ctrl+I opens the inspector: address, size, section, mangled name, and file and line from debug information.
- The status line shows machine, entry point, flash and RAM use, symbol count and problems.
- Large files load in the background, with progress. Only the needed parts of the file are read.

### Fixed

- RAM use no longer counts flash.

## alpha-2026-10-19e — 2026-10-19

### New

- Auto copy uses the Windows copy engine. File attributes and streams are kept, and shares copy on the server.
- Auto copy copies several files at once where the drives allow it, such as SSD to SSD or network. Very large files copy without buffering.
- The window opens where it was, for the current monitor setup. It always opens on a visible screen.
- Every Settings row shows Default or Changed. Reset (F8) restores the default.
- Preview: browse folders inside the preview. Enter, Backspace and the arrows move through the tree. Space marks a folder and counts it.
- When you rest on a folder, its listing loads in the background.

### Fixed

- Overwrite replaces the old file in one step. A crash or power loss no longer leaves you with neither file.

## alpha-2026-10-19d — 2026-10-19

### New

- Crash report: if Ferrite crashes, it keeps a redacted crash record. The record holds no user names, paths or server names.
- At the next start, a notice says "Ferrite closed unexpectedly last time." It has Save report and Dismiss. Esc dismisses it.
- Ctrl+Shift+R moves focus to the notice.
- Save report writes a zip file whose name starts with ferrite-report- into Downloads, and opens a tab on it. Nothing is sent anywhere.
- A hard kill or power loss also counts as an unclean exit. The notice shows even without a crash record.
- The notice shows once per crash.

## alpha-2026-10-19c — 2026-10-19

### New

- Modifier hints: hold Ctrl, Shift or Alt in the file list for 0.4 s. A drawer above the F-key bar shows that layer's shortcuts, in groups: Clipboard, Tabs, Sort, Pane, View and Window.
- The Ctrl drawer has a last column. It shows what Ctrl+Shift adds.
- A quick tap or a chord, such as Ctrl+C, does not open the drawer. Releasing the key hides it.
- If the drawer covers the cursor row, the list moves up while the drawer is open.
- Your own keys from keymap.toml appear in the drawer at once. Shortcuts that do nothing are left out.
- The drawer has a Turn off option. Settings, Appearance, Modifier hints offers All shortcuts, F-keys only (the old behaviour), or Off.
- The hint delay is a second setting, from 150 to 2000 ms.

## alpha-2026-10-19b — 2026-10-19

### New

- Only one Ferrite runs per profile. Starting Ferrite again brings the running window to the front.
- `ferrite <path>` opens a folder in a new tab in the active pane. It opens a file in a tab on its folder, with the cursor on the file.
- A missing path shows a "was not found" message. Ferrite then starts as usual.
- With Ferrite already running, the path opens in the running window.
- If the running Ferrite does not answer within 2 s, the second start prints "ferrite: the running Ferrite did not answer" and exits.
- Runs with --profile or --metrics stay separate from your normal Ferrite.

## alpha-2026-10-19a — 2026-10-19

### New

- Explorer clipboard: Ctrl+C or Ctrl+X copies or cuts the marked files, or the file under the cursor. Paste them in Explorer.
- Files copied or cut in Explorer paste into the active pane with Ctrl+V.
- Paste starts the copy or move at once. Conflict handling, progress and pause work as usual.
- After a cut is pasted, the clipboard is emptied.
- A file that another program has open names that program. For example: "report.docx" is in use by Microsoft Word.
- Shortcuts search accepts any key form. For example: ctrl+f5, F5 ctrl or Ctrl - F5.
- F3 on a folder is the only way into the preview. Ctrl+Q and the Preview button are removed. Esc closes the preview.
- The window background uses Mica on Windows 11 22H2 and later. Windows 10 and Windows 11 21H2 show an opaque window with a gradient.
- The Settings F-key bar shows F2 Edit value, F3 TOML line and F10 Menu. Shift+F10 is labelled "Context".

## alpha-2026-10-18a — 2026-10-18

### New

- Delete and Recycle dialogs: Enter and Space press the focused button. This also works after Tab moves the focus.
- ferrite.toml is safer to edit while Ferrite runs. If you delete the file and then save from the settings sheet, Ferrite writes a complete new file.
- An interrupted write is recovered at the next start.
- A hand edit to a setting is never overwritten by a retry.
- New command-line option `--left <folder>` and `--right <folder>` open the panes in that folder. A missing folder is refused with a message.
- New command-line option `--profile <folder>` runs with its own settings and cache, kept in that folder.
- New command-line option `--metrics <file.json>` writes start-up time and memory to a file, then exits.

### Fixed

- Comments at the end of ferrite.toml stay under their own section. This fixes the known issue in the previous build.

## alpha-2026-10-17c — 2026-10-17

### New

- Shortcuts page in Settings (Ctrl+, then Shortcuts). Search by name or key. Press Enter to record a new key. Conflicts show before you save, including across nested areas, such as F10. Ctrl+R resets a row. Your comments in keymap.toml stay.
- Your own keys always win over built-in keys in the same place.
- ferrite.toml lists every setting. Missing settings are added at start with their default. Values you set never change. Defaults you never touched follow future versions.
- ferrite.toml reloads live when you edit it by hand. A mistake shows its line and column. The last good settings keep working.
- Preview on a folder stays live while files change inside it. Totals update a few times per second. "Counted at" shows when they were counted. Ctrl+R recounts.
- F3 on a folder opens the preview. Esc closes it.

### Known problems

- When Ferrite adds a missing section to ferrite.toml, comments at the very end can move under the new section. A backup copy of the file from before the session is kept next to it. The fix comes in the next build.

## alpha-2026-10-17b — 2026-10-17

### New

- Your own shortcuts: put a keymap.toml next to ferrite.toml, with only the keys you change. It applies when you save, with no restart.
- `keys = []` removes a shortcut.
- A key that another command uses moves to your command. The status line says which command lost it.
- A mistake in keymap.toml shows in the status line, with its line number. The working shortcuts keep working. Ferrite never rewrites the file.
- The "Delete for good" warning on the F-key bar follows whatever key you give to delete.

## alpha-2026-10-17a — 2026-10-17

### New

- Settings sheet (Ctrl+,): each setting applies at once, or says it is not applied yet. Undo is available. F3 jumps to the matching line in ferrite.toml.
- Saves keep your comments, line endings and unknown keys. A failed save never loses your changes.
- A network share stops answering: the tab shows Offline and keeps its cached rows. It retries on its own. Ctrl+R retries at once.
- A copy or rename elsewhere no longer wipes an offline tab.
- The status shows the date when the cache is older than today.
- Live Markdown preview while you edit: Ctrl+E on a .md file, and Ctrl+Shift+V to toggle. The preview follows the cursor. F6 moves into the preview. Ctrl+click or Enter opens a link.
- Markdown tables: no column is narrower than its widest word. Tables that fit stay in the text column. Wide tables scroll sideways with Left and Right.

### Fixed

- Start-up no longer reports the disk cache as ready before its file opens.

## alpha-2026-10-16m — 2026-10-16

### New

- Right-click in F3 on a tree row or a table cell acts on the item you clicked. This covers Copy value, Copy cell, Copy column and Sort by this column.
- Retry reloads a Windows submenu that failed.
- A long right-press marks the row you pressed.
- Panes draw at once at start, from a disk cache of your open tabs' folders. They are marked as cached, then refresh.

### Fixed

- The Windows menu refuses names that Windows would rewrite: a trailing dot or space, or a device name such as NUL. A Windows action can then never hit another file. Ferrite's own actions still work there.
- Quick right-clicks no longer queue Windows menu work.
- Copy value on a huge JSON array stays fast.

## alpha-2026-10-16l — 2026-10-16

### New

- Pause and resume copies, moves and deletes. The job card has a Pause button. It shows Pausing… and then Paused. The pill shows a pause symbol.
- A paused copy never leaves a half-written file under its final name.
- Dialog keyboard focus: Tab, Shift+Tab and the arrow keys move between buttons. Enter or Space presses the focused button. The focus ring is clearer. Delete dialogs start on Cancel.
- The F3 and F4 context menu adds Find selection.
- The F3 and F4 context menu adds Copy path:line.
- The F3 and F4 context menu adds Copy value and key (tree view), Copy column (table view), Copy and Copy link (Markdown), and Copy image and Copy path (image).
- A redacted diagnostics log holds no paths or file names. Its level can be set by an environment variable.

### Known problems

- The Windows menu's Delete can target the wrong file on names that end in a dot or a space. A fix is in progress. Until it lands, avoid Delete from the Windows menu on such names.

## alpha-2026-10-16k — 2026-10-16

### New

- Copies and moves show a job card with progress, speed and time left. Ctrl+J opens it. A small pill in the F-key bar shows running jobs.
- The F3 and F4 context menu fits the file kind: text, Markdown, JSON or YAML tree, table, image, archive or hex. It offers copy, find, go to, wrap, fold and view as.

### Fixed

- A crash when a context menu action needed the window.
- Fixes in preview browsing.

## alpha-2026-10-16j — 2026-10-16

### New

- Browse inside the preview: Ctrl+Shift+Q moves focus into the preview. It opens the preview first if it is closed.
- In the preview, the arrow keys move through the folder structure and the largest files. Left and Right fold. Enter goes into a folder or selects a file in the pane. F3 views. A double-click works like Enter.
- Esc, Tab or Ctrl+Q returns to the list.
- On a file preview, focus goes into the content, so you can scroll or fold it.
- The preview keeps its place when the previewed file or folder changes on disk.

## alpha-2026-10-16i — 2026-10-16

### New

- Preview drawer: Ctrl+Q, or the Preview button in the active pane, opens a side sheet over the other pane. It shows what is under the cursor, 0.15 s after the cursor stops.
- Folder preview shows the file structure first. Folders open when you open them.
- Folder preview counts size, files, folders and last change in the background. Esc stops the count. Resume continues it. An unchanged folder answers at once.
- Folder preview shows space by type and the 10 largest files. The folder size also appears in the list.
- File preview is read-only and shows the file as F3 does.
- Tab moves the sheet to the other side. Drag its edge, or use Ctrl+[ and Ctrl+], to resize it.

### Known problems

- Browsing inside the preview is not in this build yet.

## alpha-2026-10-16h — 2026-10-16

### New

- Open panes follow changes on disk. Files created, deleted or renamed, for example by Extract here, appear within about half a second.
- The cursor and selection stay on the same names when the pane refreshes.
- Big extracts arrive in batches.
- On a share that cannot be watched, the pane refreshes when you come back to it, and every 5 s.
- Ctrl+R refreshes the active pane.

## alpha-2026-10-16g — 2026-10-16

### New

- Context menu: hold the right button on a file for about 0.4 s. You can also press the Menu key or Shift+F10.
- Ferrite's own actions come first, then the Windows items, such as Open with, 7-Zip and Send to. Type to filter the menu.
- The context menu opens in one piece, with a short motion. A long press on an unmarked file marks it first.
- The app menu opens with F10 or the "Ferrite ▾" button. It has Theme, Density and Hidden files tiles. They apply at once and are saved.
- Tabs: Ctrl+Tab and Ctrl+Shift+Tab, or Ctrl+PageDown and Ctrl+PageUp. Ctrl+1 to Ctrl+8 pick a tab. Ctrl+9 picks the last tab. Ctrl+W closes a tab.
- Panes return to the middle when you double-click the gap between them, or press Ctrl+Shift+0.
- The F-key bar is more compact, with full labels. F10 is now Menu.
- The ring around the active pane is thinner.

### Fixed

- The Windows part of the context menu no longer fails on local folders.

## alpha-2026-10-16f — 2026-10-16

### New

- Markdown in F3 is rendered: headings, lists, tasks, quotes, tables, code and links. Press 1 to see the source text.
- Files over 1 MB open as text, with a note. So do files with huge tables or thousands of footnotes.
- JSON, YAML and XML tree: Alt+Left and Alt+Right fold a branch. Ctrl+Shift+Left and Ctrl+Shift+Right fold everything.
- Ctrl+1 to Ctrl+9 show the tree to that depth. `*` and `-` still work.

## alpha-2026-10-16e — 2026-10-16

### New

- New look: the two panes are cards on a Mica background. The active pane has an accent ring.
- The app button reads "Ferrite ▾". The search box is centred. One drive pill has a dropdown.
- "+" or Ctrl+T opens a new tab.
- The F-key bar uses keycaps. Holding Shift, Ctrl or Alt lights that key and shows its layer.
- Faster folders: the native Windows listing loads folders quickly. Folders you visited come back at once, so Back is instant. They refresh in the background.
- F3 shows JSON, YAML and XML as a tree. It shows CSV and TSV as a table.

### Fixed

- The Windows "Send to" item no longer blocks.

### Built but not switched on yet

- Groundwork for the context menu. It is not wired up yet.

## alpha-2026-10-16c — 2026-10-16

### New

- The bottom bar lists the image keys: 0 Fit, 1 100 %, +/− Zoom, arrows or drag to pan, N/P, I Info.
- The mouse wheel zooms around the pointer.
- Find shows "x of y" while you type.
- Follow: Alt+F also works from the text view. It switches to the paged view at the same line.
- A "Following" badge shows when Follow is on.
- Files rewritten or replaced by an editor, such as VS Code, are picked up.

### Fixed

- Space and Backspace step to the next or previous file only while an image is shown. Elsewhere, Space pages down and Backspace does nothing.
- Holding Backspace in the archive filter no longer freezes the app.
- Stepping through files quickly loads only the last file you land on.

## alpha-2026-10-16b — 2026-10-16

### New

- Archives: F3 on a zip, 7z or tar file (gz, xz or zst) lists the contents, with sizes and dates. Type to filter. Enter opens an entry of up to 64 MB.
- Fonts: F3 on a TTF, OTF or WOFF file shows sample text in several sizes, and a glyph grid.
- Very large or hostile archives show the first 200,000 entries, or a clear message, instead of filling memory.

## alpha-2026-10-16 — 2026-10-16

### New

- Images open in F3. Zoom and pan. N and P, or Space and Backspace, go to the next or previous image. EXIF orientation is applied. SVG and SVGZ files open with safe size limits.
- Search shows "3 of 1,248". Find All lists every hit. The scrollbar shows where the hits are.
- Colour marks (Ctrl+1 to Ctrl+8) and bookmarks.
- Follow for growing log files.
- Marks, bookmarks and search history are remembered after you close Ferrite.
- Hostile files, such as huge or deeply nested images, show a message instead of freezing or crashing.

## alpha-2026-10-15e — 2026-10-15

### New

- Text files open in their real encoding. Ferrite checks the byte order mark, then an XML or HTML declaration, then UTF-8, then guesses, then uses the Windows ANSI code page.
- UTF-16 files, big-endian or little-endian, with or without a byte order mark, open as text.
- Windows-1252 and other code pages show their accents.
- The encoding shows in the viewer header.
- A file that would not save back byte for byte opens read-only. An unedited Ctrl+S never changes it.
- Files with binary data at the end, or with mixed line endings, open read-only. Editing them comes later.
- UTF-16 files over 16 MB open in Hex for now.

### Built but not switched on yet

- Groundwork: a rule lexer with Notepad++ user-defined language import. Colours for m4, OIL, DCM and community languages come in a later build.

## alpha-2026-10-15d — 2026-10-15

### New

- Syntax colours for XML (also .arxml, .csproj, .svg, .xaml and .config), SQL, PowerShell, INI, .properties, batch, Dockerfile, JSON5 and JSONC, R, Perl, LaTeX, Objective-C and assembly.
- The language is also detected from the file content. This covers an XML declaration, a shebang line such as `#!/usr/bin/env python3`, editor mode lines, and ambiguous extensions such as .m, .cls, .inc and .conf.

### Built but not switched on yet

- Groundwork for search marks: match counting, 8-colour marks, bookmarks and search history. They are saved in the local database.

### Known problems

- This build upgrades the local database to version 2. Older alpha builds refuse to open it.

## alpha-2026-10-15c — 2026-10-15

### New

- F3 opens files read-only. Ctrl+E switches to editing. Editing works for text up to 16 MB.
- Lister keys: 1 shows text, 3 shows hex. F3 or Enter finds the next match. Shift+F3 finds the previous match. n and p go to the next or previous file in the folder.
- Opening and saving run in the background. "Opening…" and "Saving…" show progress. Esc closes at any time.
- Exact file names are kept. A name with a trailing dot is never confused with the same name without the dot.

### Fixed

- Scrolling large files and hex views reads the disk in the background, so the window stays responsive.

## alpha-2026-10-15b — 2026-10-15

### New

- The viewer's keys show in the bottom bar while F3 is open.

### Fixed

- Viewer keys are no longer cut off in the header.
- Sorting no longer shifts the column headers.

## alpha-2026-10-15 — 2026-10-15

### New

- Two panes with tabs, a quick filter, and sorting with Ctrl+F3 to Ctrl+F6.
- Total Commander style selection: Space, Insert, and the Num+, Num- and Num* keys.
- Copy, move, rename (F2) and new folder (F7).
- Delete to the Recycle Bin. Shift+Del deletes for good.
- Copy, move and delete show plans, progress and cancel.
- F3 opens a built-in viewer and editor, with highlighting. Binary files open as hex. Text over 16 MB opens in pages.
- F4 opens files in the editor you pick.
- A drive bar sits above each pane. Alt+F1 and Alt+F2 open the drive menu.
- Mapped network drives reconnect.
- Ctrl+L types any path.
