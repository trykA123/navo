# Navo user guide

This guide is for people who try Navo for the first time. You do not need to know Total Commander. It lists what Navo does today. Keys are written like `Ctrl+K`.

## Contents

- [Start here](#start-here)
- [Moving around](#moving-around)
- [Finding things](#finding-things)
- [Working with files](#working-with-files)
- [Safety net and undo](#safety-net-and-undo)
- [Viewing and editing](#viewing-and-editing)
- [Layouts, panes and workspaces](#layouts-panes-and-workspaces)
- [Settings and themes](#settings-and-themes)
- [Tips for Total Commander users](#tips-for-total-commander-users)
- [Reporting a problem](#reporting-a-problem)

## Start here

Navo is a file manager that you run from the keyboard. It shows folders in panes, side by side. You copy and move files from one pane to the other. The mouse works too, but every action has a key.

Navo is in alpha. Some things are rough. Please tell us what you find.

### The 10 keys to learn first

| Key | What it does |
| --- | --- |
| `Tab` | Switch to the other pane |
| `Enter` | Open the folder or file under the cursor |
| `Backspace` | Go up to the parent folder |
| `Ctrl+K` | Open the command center: find commands, folders and files |
| `F3` | View the file under the cursor |
| `F5` | Copy to the other pane |
| `F6` | Move to the other pane |
| `F7` | Make a new folder |
| `F8` or `Delete` | Delete (to the Recycle Bin on Windows, the Trash on Linux) |
| `Ctrl+Z` | Undo the last file operation |

Two more keys help a lot. `Ctrl+,` opens Settings. `F10` opens the app menu, so you can find any command with the mouse.

If you forget a key, press `Ctrl+K` and type the name of what you want to do.

## Moving around

### Panes

Navo shows up to four panes. The active pane is the one that has the cursor. Press `Tab` to move to the other pane and `Shift+Tab` to go back. Click anywhere on a pane's header to make that pane active.

`F5` and `F6` send files to the target pane. The target has a dashed outline and an "F5/F6" tag.

Press `Ctrl+Shift+1` to `Ctrl+Shift+4` to jump to pane 1 to 4. Panes are numbered from top to bottom, then left to right.

### In the file list

- `Up`, `Down`, `PgUp`, `PgDn`, `Home` and `End` move the cursor.
- `Enter` opens a folder, or opens a file with its default app.
- `Backspace` goes to the parent folder.
- `Alt+Left` goes back. `Alt+Right` goes forward. This works like history in a browser.
- `Ctrl+R` reloads the folder.
- `Ctrl+F3`, `Ctrl+F4`, `Ctrl+F5` and `Ctrl+F6` sort by name, extension, date and size.
- `F9` copies the full path of the item under the cursor.

### Tabs

Each pane holds any number of tabs.

- `Ctrl+T` opens a new tab.
- `Ctrl+W` closes the tab.
- Every tab has an X. On the active tab it is always visible. On other tabs it shows when you hover.
- Middle-click on a tab closes it.
- Closing the last tab of a pane closes the pane. If it is the only pane, Navo leaves a fresh tab instead.
- `Ctrl+Tab` and `Ctrl+PgDn` go to the next tab. `Ctrl+Shift+Tab` and `Ctrl+PgUp` go to the previous tab.
- `Ctrl+1` to `Ctrl+8` go to tab 1 to 8. `Ctrl+9` goes to the last tab.
- `Ctrl+Shift+Up` clones the tab into the other pane.

### Path bar and drives

- Click a part of the path to jump to that folder.
- `Alt+D` edits the path as text. Type a path and press `Enter`.
- `Alt+F1` opens the drive list for pane 1. `Alt+F2` opens it for pane 2.
- You can also use the drive buttons above each pane.

## Finding things

### Quick filter

Start typing in a file list. Navo filters the list at once.

- The text can be anywhere in the name. Typing `4999` finds `file-4999`.
- The matched part is highlighted. The cursor lands on the first name that starts with your text.
- `^text` keeps only names that start with `text`.
- `*` stands for any text. `?` stands for one character.
- `;` separates alternatives. `*.jpg;*.png` shows both kinds.
- `Esc` clears the filter.

Your marks stay while you filter. The footer says how many marked items are hidden.

### Command center (Ctrl+K)

`Ctrl+K` opens one box for commands, folders and files. Nothing leaves your computer.

- Type a few letters of a command, such as "new folder". Press `Enter` to run it.
- Type part of a folder or file name. Results come in groups: Top hit, Commands, Folders and Files.
- With nothing typed, the box shows your recent commands and most used folders.
- Start with `>` to see commands only.
- Start with `/` to see files and folders only.
- Start with a drive letter, `\\` or `~` to type a path. `Tab` completes folder names.
- `ext:rs,toml` limits results to those extensions.
- `Tab` and `Shift+Tab` switch between Names and Content. Content searches inside files.

Keys in the box:

| Key | What it does |
| --- | --- |
| `Enter` | Run or open the row |
| `Shift+Enter` | Show the item in its folder |
| `Ctrl+Enter` | Do the same in the other pane |
| `Alt+Enter` | Open in a new tab |
| `Ctrl+Shift+C` | Copy the full path |
| `Esc` | Clear the text, then close the box |

Some commands ask for a value, such as Go to folder or Mark by pattern. The box turns into a step. Type the value and press `Enter`.

### Searching inside files

Press `Alt+F7` to search for text inside the files of the current folder. This opens the Content mode of the command center. You can also type your text in Content mode with `Tab`.

### Recently deleted

Type "Recently deleted" in `Ctrl+K` to see files the safety net still keeps. See [Safety net and undo](#safety-net-and-undo).

## Working with files

### Copy, move and new items

| Key | What it does |
| --- | --- |
| `F5` | Copy the marked files, or the file under the cursor, to the target pane |
| `F6` | Move them to the target pane |
| `F7` | Make a new folder |
| `Shift+F7` | Make a new file |
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Copy, cut and paste with the clipboard |

A copy or move runs as a job. Progress shows in the activity strip. You can pause or cancel a job. Press `Alt+J` to focus the strip and `Ctrl+J` to open the job list.

### Delete

- `F8` or `Delete` moves files to the Recycle Bin (Windows) or the Trash (Linux).
- `Shift+Delete` or `Shift+F8` deletes permanently. See the next section for how the safety net helps.

### Rename

- `F2` or `Shift+F6` renames the item under the cursor.
- `Ctrl+M` opens multi-rename for the marked files.
- `F2` on two or more marked files also opens multi-rename.

Multi-rename shows every old and new name in a table before anything changes. You build names from a pattern, such as `[N]` for the name, `[E]` for the extension and `[C]` for a counter. You can also search and replace, change case and clean up spaces. Problems, such as duplicate names, block the rename. The whole batch succeeds or nothing changes. `Ctrl+Z` undoes the last batch.

### Marking files

Marks pick the files for the next action. A mark is not the cursor.

- `Space` marks or unmarks the item under the cursor.
- `Insert` marks the item and moves down.
- `Shift+Up` and `Shift+Down` extend a range. `Shift+PgUp`, `Shift+PgDn`, `Shift+Home` and `Shift+End` work too.
- `Ctrl+A` marks all. `Ctrl+Shift+A` unmarks all.
- `/` (numeric keypad) brings back the marks you last cleared.
- `+` marks by pattern. `-` unmarks by pattern. `*` inverts the marks.
- `Alt++` marks every file with the same extension as the cursor file. `Alt+-` unmarks them.

With the mouse, `Ctrl+click` toggles a row and `Shift+click` marks a range. A right-button drag over rows paints marks. A left-button drag from empty space draws a box around rows.

### Drag and drop

Drag marked files, or one file, onto the other pane, a folder, a tab, a path segment or a drive button.

- With no key, a drop moves files on the same drive and copies them to another drive.
- Hold `Ctrl` to copy. Hold `Shift` to move. Hold `Alt` to make shortcuts.
- A label near the pointer says what will happen.
- `Esc` cancels the drag.
- Hold a drag over a folder for a moment and Navo opens it.

You can also drag files to and from Explorer on Windows. On Linux, files dropped from other apps are always copied.

### Right-click menu

Right-click an item to open the menu. From the keyboard, press `Menu`, `Shift+F10` or `Alt+M`. Press `Shift+F10` again inside the menu for the classic system menu.

## Safety net and undo

Navo keeps files that you delete or overwrite. This is the safety net. It is on by default.

- Deleted files and files replaced by a copy or move are kept for 3 days.
- `Ctrl+Z` brings back the last change.
- "Recently deleted" in `Ctrl+K` lists kept files, so you can restore older ones.
- Expired files are removed by themselves.

Navo keeps the files in a hidden folder on the same drive. This is not a backup. The files go after the set time, or sooner when space runs short.

You can change the safety net in Settings (`Ctrl+,`), in the section Safety net:

- Keep deleted files.
- Keep overwritten files.
- Keep for days (1 to 7).
- Space per drive.
- Stop below free space.

Some drives cannot keep deleted files. Navo tells you before it deletes, and the delete cannot be undone.

`Ctrl+Z` also undoes multi-rename, and brings back a deleted workspace.

## Viewing and editing

### The viewer (F3)

`F3` opens the file under the cursor in the viewer. Navo picks a view that fits the file.

| View | Key | Use |
| --- | --- | --- |
| Text | `1` | Plain text and code, with colours |
| Hex | `3` | The raw bytes |
| Rich | `5` | A readable view of the file type, such as Markdown, tables, trees, images, PDF, ELF and MAP files |

- `Esc` closes the viewer.
- `N` and `P` move to the next and previous file.
- `Ctrl+G` goes to a line.
- `Alt+L` shows line ends. `Alt+F` follows the end of a growing file.
- `Ctrl+Shift+E` chooses the text encoding.
- `Alt+V` opens the View menu. `Ctrl+B` and `Ctrl+I` show or hide the side drawers.
- On a folder, `F3` shows a preview. It counts the size and the files. `Esc` closes it at once, even while it counts.

### Find in the viewer

Press `Ctrl+F` in the viewer. Find works in Text, Hex, Table, Tree and Markdown views. The box shows your position, for example "3 of 17".

- `Enter` or `F3` goes to the next match.
- `Shift+F3` goes to the previous match.
- `Alt+C` turns Match case on or off.
- `Alt+Enter` lists all matches.
- `Up` and `Down` in the box bring back older searches.

### The editor (F4)

`F4` opens the file in the editor. `Ctrl+S` saves. `Ctrl+E` switches the viewer to the editor. `Shift+F4` lets you choose another program. On a folder, `F4` opens the folder in your editor.

In a Markdown file, `Ctrl+Shift+V` shows or hides a live preview.

## Layouts, panes and workspaces

### Layouts

`Ctrl+L` opens the layout switcher. Pick a built-in layout with `1` to `5`:

1. Side by side
2. Three columns
3. Wide + 2
4. Two by two
5. Main + 3

Your own layouts come after these, as `6` to `9`. Press `Ctrl+S` in the switcher to save the current layout under a name.

Applying a layout keeps your folders and tabs. A smaller layout moves the tabs of removed panes into the last pane.

### Splitting and closing panes

- `Ctrl+Alt+V` splits the pane to the right.
- `Ctrl+Alt+B` splits the pane below.
- `Ctrl+Alt+W` closes the pane.

Four panes is the most.

### Resizing

- Drag a divider to resize. Double-click it to make that split even.
- In the Two by two layout, each row has its own vertical divider. Dragging it, or pressing `Ctrl+Left` and `Ctrl+Right` in the switcher, moves only that row.
- `Ctrl+Alt+0` makes all dividers even. `Ctrl+Shift+0` also works.
- Hold `Shift` and drag a pane header onto another pane to swap them.

Windows narrower than 900 px show one pane with a strip of buttons, one for each pane.

### Workspaces

A workspace is a saved arrangement of panes, tabs and folders. Use one per task.

- `Alt+1` to `Alt+9` switch to workspace 1 to 9.
- Hold `Alt` in the file list to see a card for each workspace.
- `Alt+N` makes a new workspace from the current layout.
- `F2` on a card renames it. `Alt+Shift+Left` and `Alt+Shift+Right` move it.
- You can delete a workspace from its card. `Ctrl+Z` brings it back.

Workspaces save themselves. They come back when you restart Navo.

## Settings and themes

Press `Ctrl+,` to open Settings. Each change applies at once.

- `Up` and `Down` move between rows. `Left` and `Right` change a value.
- `Space` flips an on/off setting.
- `Enter` or `F2` edits a text value.
- `Tab` and `Shift+Tab` move between sections.
- `Ctrl+F` searches the settings.
- `F8` resets a value to its default. `Ctrl+Z` undoes a change.
- `F3` jumps to the setting in the settings file.
- `Esc` closes Settings.

Settings are stored in a plain text file, `navo.toml`. You can edit it by hand. Changes show at once.

Under Colour theme you can choose Open sea, Red bill, Black cap, Fine mesh, Grain or Ink and amber. Each has a light and a dark version. You can follow the system, or keep light or dark. A separate Syntax theme sets the colours of code in the viewer and the editor.

Other sections cover the file index for `Ctrl+K`, rename options, the safety net and diagnostics.

## Tips for Total Commander users

### What is the same

- Two panes, `Tab` to switch, `F5` copy, `F6` move, `F7` new folder, `F8` delete.
- `F3` views and `F4` edits.
- Marks are separate from the cursor. `Space`, `Insert`, `+`, `-` and `*` work as you expect.
- `Alt+F1` and `Alt+F2` open the drive lists.
- `Alt+F7` searches inside files.
- `Ctrl+F3`, `Ctrl+F4`, `Ctrl+F5` and `Ctrl+F6` sort the list.
- `Shift+F4` edits with another program. `Shift+F6` renames.
- Right-drag paints marks.
- In the viewer, `1`, `3` and `5` switch between text, hex and rich view.

### What differs

- `Ctrl+K` replaces most menus. Type what you want.
- Typing in a file list filters it. It does not jump with Quick Search.
- Editing the path is `Alt+D`. `Ctrl+L` opens the layout switcher.
- Navo has up to four panes, not only two.
- Workspaces replace saved tab sets. `Alt+1` to `Alt+9` switch them.
- Delete goes through the safety net, and `Ctrl+Z` undoes file operations.
- `F10` opens Navo's own app menu.
- `Alt+F4` quits.
- Navo is not a clone. The look is new. Settings use `navo.toml`, not `wincmd.ini`.

## Reporting a problem

Tell us what you did, what you expected and what happened. A screenshot helps.

### Crash notice

If Navo closed unexpectedly, the next start shows a notice. It offers these actions:

- `Enter` saves a report.
- `Alt+O` opens the folder with the kept data.
- `Esc` dismisses the notice.
- `Ctrl+Shift+R` brings focus back to the notice.

### Diagnostic report

Type "Save a diagnostic report" in `Ctrl+K`, or use the Help entry in the app menu. Navo saves a ZIP file named `navo-report-<date>.zip`, normally on your Desktop. It holds a readable `report.json`. You can open it before you share it.

The crash notice has its own report.

### Log files

Navo keeps a rolling log in the `logs` folder inside its profile. On Windows this is `%LOCALAPPDATA%\Navo\logs`. The log has private data removed.

If you cannot start Navo normally because the safety net or the job journal fails, start it with `--no-retention` to work without the safety net.
