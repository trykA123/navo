# Changelog

Each entry is a pre-release build for Windows. Downloads are on the GitHub releases page.

## v0.9.0 — Settings v2 complete, instant context menu, and calmer updates

*2026-11-25*

This release finishes Settings v2, makes the right-click context menu open at once, and redraws the update and workspace chrome. Find in Settings highlights matching words, counts hits per page, and searches option labels and keyboard shortcuts. Long choice rows become icon dropdowns, and About replaces its storage wall with a summary card and a collapsed details fold. Right-clicking now shows Navo's own actions immediately while Windows shell items load below them without blocking.

### Added

- **Find highlights, per-page counts, and the Changed filter in Settings.** Typing in Settings Find highlights every matched word in row labels, shows each page's hit count in the sidebar, and dims pages with no hits. A header chip (`N changed`) filters every page to modified settings with changed counts and accent dots in the sidebar, clearing automatically when the last change is undone. Pressing Esc clears Find first, then the Changed filter, then closes Settings.
- **Options and shortcuts in Settings Find.** Searching in Settings now matches choice option labels (such as "PowerShell") and keyboard shortcut names. Matching shortcuts appear under Keyboard with their current key cap and open Shortcuts directly at that command.
- **Icon dropdowns for long Settings choices.** Choices with more than three options or long labels (External terminal, Sort by, File index, Git engine) now render as a compact icon dropdown with a checkmark on the active option and full keyboard navigation instead of clipping at the edge.
- **Redesigned About Storage block.** About now shows one summary card ("Navo data: X on disk") with Open data folder and Refresh actions, keeping the per-store list behind a collapsed "Show details" fold. Store paths truncate in the middle and show their full path with Open folder and Copy path in the inspector. The per-user and machine indexes are named "Index (yours)" and "Index (all users)".
- **Quiet title-bar update pill and About Updates card.** About now presents update status in a dedicated Navo Updates card with a live download progress bar, a clear Restart to update card when ready, and both Retry and Download manually buttons on failure. In the main window, a dismissible title-bar pill tracks downloading, ready, and failed updates without vanishing on your next command.
- **Active pane hairline and calm empty/error cards.** The active file pane shows a crisp top accent hairline with zero layout shift when switching panes. Empty folders, empty filter views (with a Clear filter button), and failed folders now display calm centered cards that keep keyboard navigation working.
- **Command Center polish.** Selected Command Center rows carry a left accent bar, parent paths middle-truncate to keep both the drive and immediate folders visible, command shortcuts render as key caps, and unmatched queries offer Clear query and Search inside files (Alt+F7) recovery buttons.

### Changed

- **Instant context menu on Windows.** Right-clicking or pressing Alt+M no longer waits 150 ms for Windows Shell extensions before opening. Navo's own file actions appear immediately at the top with key caps and danger styling on Delete, while Windows Shell actions load asynchronously below a divider. Windows shell submenus defer reading their items until opened. Menus anchored near the bottom of the screen now re-clamp upward smoothly as shell items arrive.

### Under the hood

None of this can be tried yet.

- **Settings window groundwork.** Session format bumped to v7 (with full v6 migration and backup preservation) to store Settings window placements, paired with pure placement helpers and a decoupled `settings::content` builder.

### Known issues

- **Long single folder names in Command Center.** A single folder segment wider than the Command Center detail column can clip at its right edge without an ellipsis.
- **Drives without file IDs.** Same FAT/exFAT/network-share caveat as v0.8.9.

## v0.8.9 — Kept files, in plain sight

*2026-11-25*

This release shows the files a move kept. When Navo keeps a file in place to protect it, the job card now says "Completed with problems" and lists each file with its reason. Startup recovery names the copies it kept, and it finishes fast even after a very large move. Cancelling work on a slow network share stops at once. Find, the built-in terminal, the button bar and Setup get more groundwork that is not switched on yet.

### Added

- **"Completed with problems" on the job card.** A job that kept files in place no longer looks failed. Its card reads "Completed with problems" in the warning colour and stays until you dismiss it. The card lists the first three kept files, one line each with its reason, such as "Kept the original of a.txt: it changed while Navo copied it". The job pill reads "With problems".
- **Show all kept files.** Every card with kept files has a Show all button. It opens the activity drawer, which lists every kept file. Its key is Alt+J; the button shows the key you bound, or no key when the command is unbound. Esc returns focus to where you were.
- **Kept copies after a crash.** When Navo undoes an interrupted move at startup and keeps a copy, the startup notice now names each kept copy. It shows once and does not come back at the next start. The notice also lists them when Navo could not open its saved data.

### Fixed

- **Cancelling work on a network share.** A cancel that arrived just before a network read started could leave that read waiting 18 to 21 seconds for the share to time out. Navo now repeats the cancel until the read ends. Closing Navo does the same, so it no longer waits up to 2 seconds for such a read.
- **Crash rollback on drives without file IDs.** On FAT, exFAT and some network shares, Navo now reads the file at the source path the same way the copy did. Before, a rollback after a crash always kept the copy on these drives. Now it removes the copy only when the files match and their contents are the same byte for byte. Any difference or read error keeps the copy.
- **More reserved names refused.** COM0 and LPT0 now count as reserved device names in Setup's install folder check. Compare's copy checks also refuse COM0, LPT0 and the superscript forms such as COM¹.

### Changed

- **Fast recovery and undo for large moves.** Startup recovery of a crashed move with 100,000 files no longer does billions of comparisons before the window opens. Undo of a large held copy or move is fast too. Both give the same results as before.
- **Faster first SVG preview.** The first SVG preview no longer loads every system font when the picture has no text. That load stalled the first preview by 0.6 seconds or more.
- **For contributors.** The Git push test no longer fails when Windows reuses a process ID, and font and clipboard tests tolerate a slow host.

### Under the hood

None of this can be tried yet.

- **Find.** `name:` masks now filter names, and excluded words match inside names. Find has a reader for the text inside office documents. It refuses a hostile document instead of expanding it. The Find window's list draws its rows from the results and runs one search over every chosen folder in the background. Each window saves its Find window in the session.
- **A built-in terminal.** The terminal keeps a screen grid with scrollback and snapshots. It hands the screen to a waiting reader between bursts of output.
- **A button bar.** Navo can read and write its own icon pack format, with a size limit on each frame.
- **Setup in any folder.** The helper for protected folders now creates the missing folders through the folder handles it already checked. It refuses device names and odd names, and it removes what it created if a step fails.

### Known issues

- **Drives without file IDs.** On FAT, exFAT and some network shares, a move proves a source by its size, times and attributes. Windows can give a file that is deleted and recreated under the same name within seconds its old creation time. If size and modified time also match, the move cannot tell that replacement apart. Crash rollback now compares the contents too.
- **The new Settings is not complete.** The rest of Settings v2 comes in v0.9.0.
- **Setup's administrator message runs ahead.** When you start Setup as administrator, it says it was started from another account or outside your desktop. In this release, Setup refuses every start as administrator. Start Setup normally from your desktop.

## v0.8.8 — Moves that keep your files

*2026-11-25*

This release is about data safety. A move or an Overwrite can no longer delete the only copy of a file: Navo refuses to write a file over itself under another name, removes a moved source only when it can prove the copy came from that exact file, and keeps the copy when it rolls back a move after a crash. Settings shows what the index holds, and the folds for rare settings are finished. Find, the built-in terminal and the button bar get groundwork that is not switched on yet.

### Fixed

- **Overwrite onto the same file under another name.** A copy or move with Overwrite could target the source itself under a second name: a SUBST drive, a mapped drive and its network path, a mount point or a link. The move then deleted the only copy. Navo now compares the two files' identities before it writes over a target. When they are the same file, the step stops with "is the same file as its source under another name. Nothing was changed." When Navo cannot read either identity, the step also stops and changes nothing. On drives without file IDs, Navo compares the drive serial numbers, then the full resolved paths.
- **A move removes only the file it copied.** A move across drives copies each file, then removes its source. Before, if another program replaced the source after the copy, the move deleted the replacement, which was never copied. Navo now records the source's identity, size and time when the copy starts. It removes the source only while it is still that file; on Windows, the check and the delete use one open handle.
- **A kept source no longer stops the move.** When a source changed, cannot be proved or cannot be removed, Navo keeps it in place and goes on with the other sources. The move ends as kept, not failed, and each kept source is logged with its reason. A source folder that gains a new file during the move is kept as well. A move that resumes after a restart no longer blames the drive for a kept source.
- **Crash rollback keeps the copy.** When Navo rolls back a move that a crash interrupted, it now removes a placed copy only when the file at the source path is proved to be the original. Another file at that path, or a move started by an older build, keeps the copy. An old file that the copy overwrote is kept beside it.
- **PDF sandbox permissions.** Giving the PDF viewer's sandbox access to a file no longer adds duplicate permission entries or rewrites older-style permission lists. A deny entry for an unrelated account no longer forces a rewrite. Navo checks the permission data for damage before it reads it.
- **Busy folders and the clipboard.** A very busy folder no longer lets file-change events grow memory without limit. Past 4,096 changes at once, Navo lists the folder again. Clipboard history no longer misses a copy while another program holds the clipboard; it tries again shortly after.
- **Renaming a folder with Git status on.** Navo now waits until its Git helper processes have exited before a rename, so the folder is free when the rename starts.

### Added

- **Index counts in Settings.** The index page shows files and folders as separate counts and lists the indexed drives. Only fixed NTFS drives are indexed, and the counts leave out each drive's root folder. A drive that is still building shows its item count until its totals are complete. A drive that cannot be read no longer hides the others.
- **Finished folds for rare settings.** A fold row reads "N advanced settings". Its details column lists the settings inside, and Right or a click opens the fold. Find marks advanced settings with an Advanced tag.

### Changed

- **Clearer hints for search tokens.** In Ctrl+K content search, a token Navo cannot read shows a hint with an example, such as "size: needs a number, like <50MB". The hints come from the translation files.
- **External terminal, named as such.** The Open terminal here command is now Open in external terminal. Its setting is called External terminal.
- **A steadier index service.** The index service keeps one pending answer and one status update at a time, so it cannot pile up memory. It reads the details of changed files before it locks the index, so searches do not wait behind them.
- **For contributors.** Flaky tests now wait on events instead of fixed sleeps, and a slow CI host no longer times out the index service tests.

### Not switched on yet

- **Safe F5 and F6 from Find results.** Copying or moving Find results is built but not reachable yet. It checks each file's identity again before it runs, refuses a copy into the selected folder itself or into an archive, and does these checks in the background.
- **More search tokens.** Find reads `name:`, `depth:`, `word:`, `not:`, `case:`, `hex:`, `enc:`, `docs:` and `mod:7d`, and marks a wrong one with a hint. The search does not apply them yet. A query printed back as text reads back as the same search.
- **A separate Find window.** Groundwork is in: the window can remember its place, pin and queries, results free memory as they are trimmed, and match colours come from the theme. The `search.alt-f7-opens` setting is stored but has no effect yet.
- **A built-in terminal.** The engine that runs a shell is in, with per-system shell detection. Six colour schemes and the terminal settings keys are in too; the colours follow Windows contrast mode. No terminal shows in the app yet.
- **A button bar.** The groundwork for a button bar of your own commands is in: a format for icons, a runner for user commands and a stored choice to show the bar and its names. The bar does not show yet.
- **Setup in any folder.** More groundwork is in: Setup started as administrator can start Navo as your own account, and the helper for protected folders checks every parent folder before it creates anything. Setup still installs for one user only.

### Known issues

- **Kept sources show as one message.** The jobs panel shows only the first kept source's reason. The full list is in the job log.
- **Drives without file IDs.** On FAT, exFAT and some network shares, Navo proves a file by its size, times and attributes. Windows can give a file that is deleted and recreated under the same name within seconds its old creation time. If size and modified time also match, Navo cannot tell that replacement apart.
- **The new Settings is not complete.** The rest of Settings v2 comes in v0.9.0.
- **Setup's administrator message runs ahead.** When you start Setup as administrator, it says it was started from another account or outside your desktop. In this release, Setup refuses every start as administrator. Start Setup normally from your desktop.

## v0.8.7 — Settings, rearranged

*2026-11-24*

This release starts the new Settings: ten pages, one line per setting with a details column, folds for rare settings, page keys and Undo for each change. The modifier hints become a ribbon you can also open with the mouse, and the file list repaints faster while a filter is on. Setup gets the groundwork for installing in any folder, Program Files included, but that part is not switched on yet. v0.9.0 switches it on and completes the new Settings.

### Added

- **Ten Settings pages.** Settings now has ten pages: Appearance, Window, Files, Copy and delete, Search, Viewers, Tools, Tray and clipboard, Keyboard and About. Compare sits under Tools, and Diagnostics and Help share About. Shortcuts moved into Keyboard, behind a Show shortcuts button. A greyed-out Modules page marks where modules will appear later.
- **Page keys.** Ctrl+Tab or Ctrl+PgDn goes to the next page, and Ctrl+Shift+Tab or Ctrl+PgUp to the previous one. Ctrl+1 to Ctrl+9 and Ctrl+0 jump to a page; hold Ctrl to see each page's digit. Down from the last row of a page moves on to the next page.
- **One line per setting, with a details column.** Each row shows only a changed dot, the name and the control. The column beside the list shows the focused setting's value, its default once changed, help, pictures, Reset to default (F8) and its `navo.toml` key with Open the line (F3). In a window narrower than 960 px, the column moves under the list.
- **Nested settings.** A setting that depends on another sits indented under it, with a guide line. For example, Descending sits under Sort by. While the parent is off, the nested row is dimmed and its value does not change.
- **Folds for rare settings.** 14 rare settings sit in a closed "N advanced settings" fold at the end of their group. Right or a click opens a fold, and Left closes it. Find still reaches every advanced setting and opens its fold.
- **Undo after each change.** After a change, the status line names the setting and offers Undo. The text edit bar has Save and Cancel buttons. You set the tray keys (summon, clipboard) by pressing the key you want.
- **The modifier ribbon.** Holding Ctrl, Shift or Alt shows a card with that modifier's keys in named groups, such as Files, Tabs and Window for Ctrl. Each key has a short label, and the full command name is its tooltip. The header counts the keys, names the layers one modifier away ("+ Shift 10 more") and has a Turn off button. The three cards share one height, so the card does not jump when you switch modifiers. Delete permanently is marked as dangerous. F1 to F10 get their own row while the F-key bar is hidden.
- **The ribbon by mouse.** A click on the Shift, Ctrl or Alt pill in the F-key bar pins that card open. A second click on the pill, Turn off or Esc closes it. Pinning needs modifier hints turned on.

### Changed

- **A faster file list filter.** While a filter is on, each repaint works out a row's display name once and shares one work buffer across all rows. The rows look the same as before.
- **Updates stay off for a folder you cannot write.** When your account cannot write the install folder, Navo turns updates off and logs why. Before, it asked at every start.
- **Faster checks for contributors.** The windows-quick check runs about 1.5 to 2 times faster: it builds the tests once and runs them on four machines at the same time. Every job has a time limit, and release builds use the locked dependency versions. Reusable workflows now live in the `trykA123/ci` repository.
- **The repository holds only the product.** Plans and design documents moved to a separate repository.

### Not switched on yet

- **Setup in any folder.** The groundwork to install Navo in any folder, Program Files included, is in. It covers Setup started as administrator from your own desktop, a helper that creates protected folders after a Windows prompt, and new folder checks. The new folder rules are still off. Setup still installs for one user and still refuses a start as administrator. v0.9.0 switches this on.
- **Removing the PDF sandbox on uninstall.** The step that deletes the PDF viewer's Windows sandbox profile is built. A fix makes it accept only the answers Windows really gives when the profile is already gone. Uninstall does not run this step yet.

### Known issues

- **The new Settings is not complete.** A fold row shows only its arrow and count, and Find does not tag advanced settings yet. The rest of Settings v2 comes in v0.9.0.
- **Setup's administrator message runs ahead.** When you start Setup as administrator, it says it was started from another account or outside your desktop. In this release, Setup refuses every start as administrator. Start Setup normally from your desktop.

## v0.8.6 — A sharper F3

*2026-11-24*

This release makes the F3 editor a real workbench: regex find and replace, Filter Lines, lint marks for structured files, soft wrap, saving in another encoding, a hex inspector, an image eyedropper and a query bar for JSON trees. Archives now open right in the pane. Settings opens as a dialog, PDF previews run in a sandbox, the index service answers faster, and Ctrl+K stays quick with many results.

### Added

- **Regex find and replace in the editor.** The find bar now works while you edit. Alt+R turns on regex mode; Replace fills in `$1`-style groups, and Replace all is one undo step. On large files the match count runs in the background and stops at 100,000, shown as "100,000+".
- **Filter Lines.** A "Filter lines" menu on the find bar, and four Ctrl+K commands, keep, remove, copy or bookmark the lines that match the find pattern. Keep and Remove are one undo step each, and every line keeps its own line ending.
- **Lint marks for structured files.** JSON, TOML, YAML, XML and CSV files show their problems as squiggles in the theme's error and warning colours. F8 and Shift+F8 jump between them, and the header counts them.
- **Soft wrap you can turn off.** Alt+Z, the View menu or Ctrl+K turns soft wrap off and on while you edit. The choice holds for the window.
- **More cursors.** Ctrl+Alt+Up and Ctrl+Alt+Down add a cursor above or below.
- **Save in another encoding or line ending.** The encoding picker can save the file as another encoding, or with LF or CRLF line endings. Navo refuses the save, and writes nothing, when a character would not read back the same, for example ¥ in Shift_JIS.
- **A hex inspector.** In hex mode, the arrow keys or a click move a byte cursor, and Ctrl+I opens a drawer that reads the bytes there as integers (both byte orders), floats, UTF-8 and UTF-16 characters, a Unix time and bits.
- **An image eyedropper and pixel grid.** E turns on the eyedropper: the status line shows the colour under the pointer as Hex, RGB and OKLCH, and a click copies it. Past 800 % zoom, a grid shows the pixel edges.
- **A query bar for JSON and other trees.** Ctrl+Shift+F in the tree view opens a query bar (`$.store.book[0]`, `..name`, `[*]`). Enter and Shift+Enter step through the matches, and a breadcrumb shows the selected node's path; click a part to select it.
- **Archives in the pane.** Enter or a double-click opens a zip, 7z or tar file as a folder. F5 or a drag copies entries out. Archives stay read-only, and every write says why it is refused.
- **Allowlisted tools for the editor (not in the menu yet).** The engine that runs a command-line tool on the open file is in: no shell, `.bat` and `.cmd` refused, tools from inside a repository refused, a 5 s limit and a 16 MiB output cap.

- **An outline of the file.** Ctrl+Shift+O in the F3 editor lists the file's symbols; type to filter, Enter or a click jumps to the line. The View menu lists it too, and the outline is parsed in the background.
- **Checksums and strings in the hex inspector.** The inspector shows CRC32, SHA-256 and BLAKE3 of the whole file and a list of the strings in it.

### Changed

- **Leaving a folder stops its listing.** Leaving, replacing or closing a folder that is still loading cancels its job, even a network read that is stuck in a call.
- **The index service creates a missing change journal.** On a selected drive without a USN journal, the service creates one once instead of failing.
- **Clearer trust wording.** The index service row in Settings says what the index lists and who can search it. Clipboard secrets are described as protected by your Windows sign-in, and readable by other programs you run.
- **The index service log says more.** The heartbeat counts busy replies, and the "snapshot restored" line says how long the load took.
- **Under the hood.** Two new build checks stop the number of unbounded channels, lint exceptions and sleeps in tests from growing.

- **Settings opens as a dialog.** Settings floats in the middle of the window over a dimmed background, at most 1040 × 800. Esc, the close button or a click outside closes it, and focus goes back to the pane you were in.
- **PDF previews run in a sandbox.** The PDF helper now starts in a Windows AppContainer that can read only its own files. When the sandbox cannot be set up, the preview says "cannot open safely" instead of opening the PDF unprotected. An install on a FAT or exFAT drive (a portable USB stick) cannot set the sandbox up, so it cannot preview PDFs.
- **A faster, steadier index service.** Each drive gets its own wait for a search answer within one 120 ms budget, so one busy drive no longer slows the others. Profile lookups are cached per drive (from about 0.7 ms to a few microseconds per request). A failed rebuild keeps the last good index, and after the change journal wraps twice in a row the service keeps the new index and checks it in the background.
- **Folder sizes stop when you leave.** Leaving a folder cancels its size calculation between requests, so the next folder's sizes start at once.
- **Finished copies count what really happened.** The finished-job text counts only the items really placed and says how many the conflict sheet skipped.

### Fixed

- **Overwrite choices tell the truth about Undo.** When the safety net does not keep overwritten files, the copy and move dialog says "Overwritten files cannot be undone." under the choices.
- **No more half-finished moves over a folder.** Overwrite older and Overwrite all now skip a folder, or a name that is a file on one side and a folder on the other, instead of planning an overwrite the job then refused partway.
- **Conflict counts in other languages.** The conflict lines take "item" or "items" from the message files, so translations can use their own plural forms.
- **Large-file swap lands on the right line.** When the editor takes over from the paged view, it now lands on the right top line in UTF-16 and other non-UTF-8 files, and ignores a late frame from the previous file.
- **Ctrl+K no longer lags with many results.** Holding an arrow key in Ctrl+K with hundreds of results moved the selection slowly; merging the results is now about a hundred times faster.
- **Index search never shows other users' folders by mistake.** When the service cannot tell whose profile a folder belongs to, it hides that folder instead of showing it.
- **Closing Navo no longer waits on a stuck network drive.** A cancelled job on a drive that is still being identified is dropped unrun, and closing waits at most 2 s for background work.
- **Closed tabs let go of their folder.** A closed tab no longer keeps its last folder loaded in the background.
- **Lint marks on large files appear sooner.** The editor maps all problem positions in one pass.

## v0.8.5 — Your call on conflicts

*2026-11-24*

This release lets you choose what a copy or move does with names already there. It also guards the safety net on full drives, leaves online-only files in the cloud, and puts the PDF viewer in a tighter box.

### Added

- **A choice for names already there.** When a copy or move meets files with the same names, the dialog shows one row: Skip, Overwrite older, Keep both or Overwrite all. Click a choice, or use Tab and the arrow keys, then Enter. The line above the row says what will happen, for example how many items are skipped or overwritten.
- **A "Show conflicts" list.** The link opens a list of the first 200 clashes in the dialog. Each entry shows the name, then the size and date of the file being copied and of the file already there. Arrow keys, Page Up, Page Down, Home and End scroll it. Esc or Back return to the dialog.
- **A warning before you replace files that cannot be kept.** When the target drive cannot keep the files you replace, the dialog says so before you confirm. Navo shows no warning when the safety net is off, the same as for a delete.
- **WSL distributions in the drive list.** Installed WSL distributions appear after the drive letters, as `\\wsl.localhost\<name>\`. Listing the drives never starts a stopped distribution, and Navo never indexes these entries for search.
- **A cloud mark for online-only files.** A OneDrive file that is not on this PC shows a quiet cloud after its name. The tooltip and the screen reader say "Online-only file — opens after download".

### Changed

- **The safety net on an almost full drive.** Low free space never removes an item kept in the last 24 hours. On a drive below the "Stop below free space" minimum, Navo keeps no new deleted files. The delete dialog says the drive is almost full, and the delete runs without a copy only after you confirm.
- **Locked files are tried again.** When another program holds a source file open, the copy tries again after 1, 2 and 4 seconds. Only then does it show the error. A cancel during a wait stops the job at once.
- **Quick view never downloads online-only files.** Quick view does not read a cloud file by itself. It shows "Cloud file — open to download" instead.
- **Large text files open at once.** A text file above 1 MiB opens in the paged Text view straight away. The editor loads in the background and takes its place on the same top line, with no jump. A Ctrl+E pressed meanwhile starts editing after the swap.
- **Folder sizes come from the index in one go.** When the list shows folder sizes, Navo asks the search index about all shown folders together, up to 64 per request. Folders the index cannot answer fully are still measured one by one.
- **Portable mode stays in its folder.** A portable Navo keeps its run files inside its own data folder, and a test proves that a portable start writes nothing to the temp, app data or home folders.
- **The PDF viewer gets a tighter box on Windows.** Navo opens the PDF itself and hands the viewer helper a read-only handle; the helper never gets a path. The helper starts paused inside a Job Object, so it never runs unconfined. The job blocks the desktop, the clipboard, other programs' windows, global atoms and system settings. The helper also starts with strict handle checks, no extension points, no remote images and no generated code.
- **Under the hood.** A build guard keeps the compare app's parts and the main app's parts from depending on each other the wrong way. Research picked the parts for a future terminal drawer.

### Fixed

- **Viewing a PDF no longer blocks saving it.** Another program can save the file while Navo shows it.
- **Quick view stays responsive in a busy folder.** A burst of scan updates keeps only the newest one waiting, so the window no longer falls behind.

### Not switched on yet

- **A copy speed cap.** Copy jobs can run under a speed limit in bytes per second, but there is no setting for it yet.

## v0.8.4 — Copies you can trust

*2026-11-23*

This release checks copies. Navo can now read each copied file back and compare it with the original before it counts the copy as done.

### Added

- **Verify after copy.** After a file is copied, Navo reads it back from the target and compares it with the source. A file that does not match fails that step, and a move keeps its source file. The check reads past the Windows file cache, so it sees what really landed on the drive.
- **A "Verify after copy" setting.** Settings has a row with Auto, Always and Off. Auto checks copies to USB drives and network shares and skips fast internal drives.
- **A switch in the copy and move dialog.** The dialog shows whether this copy will be checked, and a click, Tab and Space flip it for this job only.
- **The job card shows the check.** While a copy is checked, the card says "Verifying…". At the end it says "Done · Verified n files", or "n of m files" when some files could not be checked.

### Changed

- **Copies between different places are checked as they stream.** For a copy out of an archive, a Git history or files pasted from another app, Navo takes a fingerprint of the bytes while it reads them. It then compares the placed file with exactly what it read. A file that the source program writes itself counts as not checked.

## v0.8.3 — Your search, your files

*2026-11-23*

This release makes search respect other people's files and makes the installer check what it installs.

### Fixed

- **Search no longer shows other people's files.** On a PC with several Windows accounts, Ctrl+K could list files from the other accounts' profile folders. The index service now hides them, also when a folder is reached through a junction. Your own profile and the shared Public folder stay searchable.
- **The profiles folder is read correctly.** When Windows stored that path with a variable in it, the service read a few stray characters past its end and looked in the wrong place.

### Changed

- **The installer checks the index service.** Navo knows the fingerprint of the service it ships with. An install now refuses a service file that does not match and tells you why.
- **Under the hood.** Slow tests get more time on the build machines, so a busy machine no longer fails a release by mistake.

## v0.8.2 — The index stays put

*2026-11-23*

This release keeps the search index calm on a busy Windows drive. Before, a file deleted while a program still had it open made the index service rebuild the whole drive every 7 to 10 minutes. The index also did not save while a rebuild waited, so each restart built the drive again from scratch. Both are fixed, and the fixes passed their checks in the Windows VM.

### Fixed

- **Files deleted while open no longer force a rebuild.** Windows parks such a file in a hidden system folder until the last program closes it. The service now removes the file from the index and carries on.
- **A restart reuses the index.** The service saves the index while a rebuild waits. After a restart, Ctrl+K works at once instead of waiting for a new full build.
- **Changes in skipped folders no longer force a rebuild.** A full build leaves out some system folders. New files there are ignored, and a folder moved there leaves the index with everything inside it. Only a change the service cannot prove safe waits for one rebuild.
- **Fewer full rebuilds.** A file created and deleted a moment later no longer forces a rebuild. A change to a file with several hard links re-reads only that file.
- **No lost subfolders.** In v0.8.1, a folder deleted right after a subfolder moved out of it could drop that subfolder from the index. The service now keeps the order of changes, or rebuilds when it cannot.
- **Settings no longer sticks on "Updating".** When the change journal cannot be read, Settings shows the true state of the index.
- **Settings shows the service memory again.** The app refused the service's memory answer, so the value stayed empty.
- **A stuck change journal is handled.** The service treats a journal that stops moving as lost. It rebuilds once and logs the problem once, not over and over.
- **A silent client no longer blocks the index.** The service drops a connection that sends nothing for 30 seconds and serves the next one.

### Changed

- **The index keeps up faster.** The service reads changes without a pause while they arrive. It waits half a second when the drive is quiet, and from 2 seconds to 5 minutes after errors. On Windows it reads the change journal in larger blocks.
- **The index stays searchable after an error.** A failed journal read, or a batch of changes that cannot be applied, keeps the current index. The service waits for one rebuild and keeps saving meanwhile.
- **The setup wizard fades between pages** instead of sliding.
- **Empty folders say so.** A finished folder with no files shows "This folder is empty". A filter that hides every file shows "No files match the filter. Esc clears it."
- **Settings shows the index build percentage** for each drive.
- **Settings is tidier.** Rows no longer carry a wrong "Not applied yet" tag. Settings that no longer do anything are hidden, and their saved values are kept.
- **Large copy and move jobs plan and resume faster.** Each step checks the plan and the finished steps at once instead of scanning the whole list. A damaged job journal can no longer make a resumed job reserve too much memory.
- **The delete safety net is explained better.** The free-space setting now says what it does.
- **Security update.** The URL library moves to 2.5.9, which fixes a crash on deeply nested addresses.
- **Under the hood.** Each release download carries a record of how it was built, two more speed benches can fail the build, two empty components are gone, and the checks are stricter.

## v0.8.1 — The index finishes again

*2026-11-19*

This release fixes the search index on busy drives. Since v0.7.1, the index service could start building a drive over and over and never finish, so Ctrl+K found nothing.

### Fixed

- **The index finishes on a busy drive.** A full build reads folders first and files second. Before, a folder or file that changed between the two reads made the service throw the build away and start again, every few seconds, forever. Now the build keeps going, places late changes at the end, and the change journal fills in the rest.
- **A failed build waits before it tries again.** The wait grows from 2 seconds to 5 minutes, so a drive that cannot be read no longer keeps the processor busy.
- **Rare moves during a build are checked later.** If folders moved in a loop while the build ran, the service builds that drive once more after 10 minutes, beside the working index. Search keeps working meanwhile.
- **The service log says more.** A restart names its cause, and a finished build counts the records it skipped.

## v0.8.0 — Colours everywhere

*2026-11-19*

This release brings syntax colours to every text file the viewer opens: huge XML and JSON, logs, automotive files and your own Notepad++ languages. A new picker lets you choose the language yourself.

### Highlights

**Big files get syntax colours.** An ARXML, XML or JSON file over 16 MiB opens in the paged Text view with colours: tags, attributes, values, comments, keys, strings and numbers. Scrolling stays as fast as plain text, and memory stays the same. Set `NAVO_SYNTAX=off` to turn colours off.

**Logs and plain text get colours.** In `.log`, `.txt` and other text files, Navo colours level words (ERROR, WARN, INFO, DEBUG), dates and times, numbers and hex, quoted text, paths, links and GUIDs. This works in F3, in Ctrl+E while you type, and in files of any size. A line over 10,000 characters stays plain in the editor.

**More files are recognised.** Navo tells XML and JSON apart by their content when the extension says nothing, including UTF-16 files. CDD and ODX diagnostic files open as XML. A file that looks binary stays plain.

**Automotive files get colours.** A2L, DBC, LDF, LSL and TASKING map files are coloured in F3 and Ctrl+E, at any size. Navo also reads your Notepad++ user-defined languages from `%APPDATA%\Notepad++\userDefineLangs`, and its own rule files from the profile's `config\languages` folder. A broken or oversized language file is skipped and logged.

**Choose the colours yourself.** Ctrl+Shift+L, View → Language… or the Syntax button in the viewer header opens a language picker: Auto, Plain text, Log, or any built-in or user language. The choice stays while you switch views or follow a log.

## v0.7.1 — Clicks that do what you mean

*2026-11-18*

This release fixes the mouse. A typed path now closes when you click away. Shift+click selects a range again. A small shake of the mouse no longer starts a drag. Setup also shows a real progress bar and starts Navo after the install.

### Highlights

**The path bar closes when you leave it.** You type a path into the path bar and then click elsewhere, switch to another tab, or go back or forward. The typed path now closes. Before, it stayed open. Right-click → Copy on the typed path still copies the path.

**Shift+click selects a range again.** The Shift hint drawer no longer swallows the click. Hold Shift, click a second file, and every file between the two is selected.

| Try it | Key |
| --- | --- |
| Select a range | Shift+click |
| Add or remove one file | Ctrl+click |

**A small mouse movement no longer starts a drag.** While you Shift+click or Ctrl+click, Navo starts a drag only after the mouse moves 6 px. Before, a tiny movement could drag a file you only meant to select.

**Setup shows its real progress.** The Installing page fills a bar and shows "NN %" beside the elapsed time. If Setup cannot read the install log, the bar keeps its soft pulse. It never freezes at a made-up number.

**"Open Navo after install" works.** Before, Navo did not start, and the next start showed "Navo closed unexpectedly last time". Now Navo starts after Setup, and no crash notice follows. If a start fails, the reason goes to the Setup log.

### Also new

- A first full build of an NTFS drive index peaks at about half the memory. At 2 million files the peak falls from 3.29 to 1.61 times the index size. At 5 million files it falls from 3.28 to 1.69 times.
- The index builds in two passes. It reads the folders first and then the files, so no file waits for its parent folder.
- In compare, a file from a Git repository whose content is not downloaded yet (a partial clone) now reads "missing content". Before, it could look like an empty file.

### Fixed

- Setup: before, choosing a folder under Program Files said "Setup cannot write to this folder". Now it says that folder needs administrator rights, which Setup cannot ask for yet.
- Search index: a hard-link change on a drive now makes the service read the folder list again before it refreshes sizes.
- Security: Velopack is updated to 1.3.39. This closes two published advisories for the zip library.

### Built but not switched on yet

- An old index that stays searchable while a new one builds. The pieces are in, but the service does not use them yet.
- A "waiting" card for folder sizes while an index rebuilds. The card is drawn, but nothing shows it yet.
- Compare reports its phase (listing, quick check, content check) and slow files. Nothing shows it yet.

### Known issues

- Setup is not signed yet, so Windows may warn about the file.

## v0.7.0 — Read every line, install like an app

*2026-11-12*

You open two versions of a config file. The changed lines carry a soft colour, and the words that changed inside them stand out. The code is coloured as it is in the viewer. A long line scrolls sideways, with both sides moving together, or wraps when you press Alt+Z. Alt+↓ brings the next change to the middle of the screen.

### Highlights

**Compare now says what changed, in plain words.** Between the two folder panes, each row names its result: changed, only on one side, or "3 inside" for a folder with differences below it. A missing file says "not in left" or "not in right" on a hatched row. The newer side shows its size and date in bold with a ▲ mark. A thin change map runs along the edge of the list and shows where the differences are.

| Try it | Key |
| --- | --- |
| Compare the two panes | Ctrl+Shift+D |
| Open a file pair as a diff | Enter |
| Next or previous change | Alt+↓ / Alt+↑ |
| View menu | Alt+V |

**Long lines no longer run off the screen.** In a file diff, lines do not wrap by default. Both sides scroll sideways together, so matching text stays level. Line numbers, edge bars and the change map stay where they are. If you prefer wrapping, one key turns it on, and both sides keep their rows aligned. Navo keeps your choice until you close the compare screen.

| Try it | Key |
| --- | --- |
| Scroll sideways | Left / Right |
| Scroll sideways with the wheel | Shift + mouse wheel |
| Scroll sideways with the mouse | Drag the bar under each side |
| Wrap long lines on or off | Alt+Z |
| Wrap from the menu | View → Wrap lines (the title-bar View button opens it too) |

**Navo now has a proper installer.** `Navo-Setup.exe` walks you through five pages: Welcome, Licence, Install folder, Shortcuts and Finish. It installs for you alone and never asks for administrator rights. It works offline. If Navo is already installed, it updates it in place and keeps your settings and shortcuts. To remove Navo, use Windows Settings → Apps.

| Try it | Key |
| --- | --- |
| Press the main button | Enter |
| Go back or next | Alt+B / Alt+N |
| Close before the install starts | Esc |

### Also new

- The file diff and the Stream draw code with syntax colours. A very large file stays plain.
- The Stream wraps long lines with Alt+Z. Its cards do not scroll sideways.
- Setup picks a safe folder. It always makes a new `Navo` folder, and it protects that folder so only you, the system and administrators can change it.
- Setup warns you when other people on the PC could change a folder above the one you chose. You can still go on.
- Setup refuses a folder it cannot install into safely, with one line that says what to do.
- Names with numbers sort the way you read them: `file2` comes before `file10`.
- Binary files open as a hex diff in compare. Alt+V switches between Text and Bytes.
- Navo is now licensed under MIT or Apache-2.0, at your choice. The Setup Licence page shows the text.

### Fixed

- Search index: before, Navo could scan a whole drive a second time while the background service already indexed it. Now it leaves that drive to the service.
- Folder refresh: before, a refresh of a large folder could use a lot of memory. Now it uses much less.

<details>
<summary>Every change</summary>

#### Compare: folders

- A narrow column between the panes names each row's result: `changed`, `◀ only`, `only ▶`, `type ≠` (a file against a folder or a link), `3 inside`, `same*` (the same once your compare rules apply) or a percentage while Navo still checks.
- Hover `3 inside` to read "contains differences".
- A missing item reads "not in left" or "not in right". A row Navo has not checked reads `…` or "unchecked". A row it cannot read shows "can't read".
- When both sides exist, the newer side has its size and date in bold with a ▲ mark. The older side is muted.
- The old badge column is gone. The gutter replaces it.
- The change map beside the right pane marks where the differences are in the whole folder.

#### Compare: file diff and Stream

- Changed lines have a coloured background and a bar on the edge. The colours are easy to read in both light and dark themes.
- Changed words inside a line have a stronger fill.
- Syntax colours match the viewer. A very large file stays plain. A reload of the same pair keeps its colours until the new ones are ready.
- Left and Right scroll sideways. So do Shift + wheel, a touchpad swipe and the bar under each side.
- Alt+Z, or View → Wrap lines, wraps long lines. A wrapped pair is as tall as its taller side. Press Alt+Z again to turn it off.
- While lines wrap, Alt+↓ and a click on the change map put the change in the middle of the screen.
- On the compare screen, the title-bar View button opens the compare View menu.
- The hex view never wraps and never scrolls sideways.
- The Stream wraps its text cards with Alt+Z. Its cards never scroll sideways.
- Ctrl+Home and Ctrl+End jump to the ends of a very large text file.
- In a large diff, a long line over 1 MiB offers "Show bytes here".
- Ctrl+I shows details for large, binary and image pairs. A memory limit in Settings can lower detail for huge changes, and a link in the note opens that setting.

#### Installer

- Welcome says "Install Navo" and the version. If Navo is installed, it names the folder and says Setup updates it there.
- Licence needs a tick on "I accept the licence" before Next.
- Install folder shows the space needed and the free space. Browse picks a parent folder.
- Shortcuts offers Start menu and Desktop, both ticked. An update skips this page and never makes a shortcut again.
- Installing shows the elapsed time. After 3 minutes it says "Setup is still working…" and names the log.
- Finish says "Navo is installed." and shows the folder on its own line.
- Setup refuses: a path with `%`, `^`, `&` or `!`; a folder that exists; a link, `subst` or network drive; a system, profile or OneDrive folder; a drive with less than four times the install size free; a folder it cannot write to.
- Setup warns, but still installs, for a folder made by a user directly under `C:\`, a drive root that lets others delete or rename its contents, and FAT32 or exFAT drives. A folder directly under `C:\`, such as `C:\Navo`, works for a standard user.
- Only one Navo installs at a time. The same or a newer version already installed: Setup starts nothing. To go back to an older version, uninstall first.
- If Navo runs from the folder, Setup asks you to close it. It never closes Navo itself.
- If you start Setup as administrator, it asks you to start it again normally.
- If the install ends but Setup cannot confirm it, it sends you to Windows Settings → Apps. It never offers to delete the folder.
- `Navo-Setup.exe --silent` installs without a window. Options: `--installto <new folder>`, `--no-desktop` and `--no-start-menu`.
- Uninstall deletes the install folder and every shortcut that points into it. Your settings and data folders stay. It removes the index service after one administrator prompt.

#### Search and sorting

- The app skips a drive the background service already indexes, also when the service only lacks file sizes.
- A refresh of one folder peaks at much less memory.
- Digits in names sort by value at any length. Leading zeros break a tie last.

#### Security

- The Git library and the SQLite library are updated. One update closes a published Git security advisory.

#### Known issues

- Setup is not signed yet, so Windows may warn about the file.

</details>

## v0.6.0 — 2026-11-10

### Added

- In a file diff or the Stream, Ctrl+Alt+← and Ctrl+Alt+→ copy the current change to the left or right side. A bar on hover offers the same. The change then says "Copied to the right · Undo", and Ctrl+Z puts the file back byte for byte.
- If a file changed after Navo showed it, the copy is refused with "Changed since shown" and the card reloads. A Git or read-only side says why it cannot be written.
- `/` searches the Stream. It narrows the cards to the files and changes whose changed lines or path contain the text, and the outline shows hit counts. J and K move between matches only, and Esc clears the search.
- With a search active, Ctrl+Shift+← and Ctrl+Shift+→ (or "Copy all N") open a plan sheet. It lists each file, its matching changes and anything that cannot be copied. Enter copies them all as one job, and one Ctrl+Z undoes every file.
- You can copy only some lines of a change. Shift+↑, Shift+↓, Space, the tick boxes on hover or Shift+click pick lines on one side; Ctrl+Alt+← or → then copies just those. Ctrl+Shift+Del removes the picked lines of a one-sided change. Esc clears the pick.
- Right-click on a line of the diff or the Stream opens a menu with every action for that line and its key. Shift+F10 and the Menu key open it under the cursor.

### Fixed

- Git badges no longer blink. Before, a recheck of a repository that had not changed dimmed its badges for a second and brought them back, which could repeat every few seconds.

### Notes

- Copies use the same safe save as other file jobs: they show in the job list and can be undone from Undo.
- Binary files, images and text files over 64 MiB still show a summary card. Image compare comes in a later version.

## v0.5.0 — 2026-11-10

### Added

- When you compare two folders, Enter on a file pair opens its diff in the full window. The two files sit side by side, or in one column when the window is narrow. Alt+↓ and Alt+↑ (or F7 and Shift+F7) move to the next and previous change, and Esc goes back to the folders with the file selected.
- A change map runs down the right edge of the diff. It marks every change, and a click jumps to it.
- In the Rich look, the words that changed inside a line are highlighted.
- Alt+V opens the View menu: Simple or Rich, side by side or one column, changed words and the change map. Ctrl+B lists the changes, and Ctrl+I shows each file's encoding and line endings. Both panels start closed.
- Alt+F opens the Stream: every difference under the compared folder, one card per file, in path order. J and K move from change to change across files, Shift+J and Shift+K move from file to file, O folds a card, and Enter opens the file at that change. Esc from the file comes back to the same card.
- V marks a file as Viewed and moves on. A bar at the bottom counts the viewed files. Viewed marks are kept by file content, so they survive a refresh and a restart, and a file that changes again shows up as not viewed.
- Ctrl+B in the Stream opens an outline of the files, grouped by folder, with their status and Viewed marks. Enter or a click jumps to a file.
- Ctrl+Alt+↓ and Ctrl+Alt+↑ in a file diff move to the next and previous file of the Stream.

### Notes

- The Stream builds the cards near the view first and keeps their memory within a quarter of the memory budget in Settings. Cards far from the view are rebuilt when you come back to them.
- Binary files, images and text files over 64 MiB show a short summary card in this version. F3 opens them in the viewer.
- Copying changes from one side to the other comes in the next version.

## v0.4.1 — 2026-11-09

### Fixed

- Renaming, deleting or moving a folder that holds a Git repository works while Git badges are being checked. Before, Windows could say the folder was in use. The same holds for any folder above or inside a repository.
- While the rename or delete runs, the repository's badges keep their last state, then refresh about half a second later.
- A background fetch, or a Pull or Push you started, is never stopped. Navo waits up to 3 seconds for it to end; if it still runs, the operation says the folder is locked by `git.exe`, and Retry works once Git ends.

### Security

- The built-in SQLite is updated to 3.54.1, which fixes RUSTSEC-2026-0459: a crafted database file could return stale memory to queries.

## v0.4.0 — 2026-11-09

### Fixed

- You can rename or delete a Git repository folder while Navo shows its Git badges. Before, Windows said the folder was locked.

### Changed

- The search index service runs as its own program, `navo-index.exe`, shown as "Navo Index" in Task Manager. Updating from an older version moves the service with one administrator prompt.
- Removing the index service leaves no Navo folder in Program Files, unless you put other files there. A file that Windows will not let go is deleted at the next restart, and Settings says so.
- Installing or removing the service is safer: one change runs at a time, a failed install puts the old service back, and a slow service gets time to stop.

## v0.3.0 — 2026-11-05

### Faster

- Large folders share their rows across tabs, sorts and filters, making navigation faster and using less memory.
- Git badges reuse a recent status while watched repositories stay unchanged. Refresh and Git commands always check again.
- File search uses a smaller index and less memory while rebuilding it.

### Fixed

- Renaming a file in an open Linux folder no longer occasionally looks like deleting one file and adding another.

## v0.2.3 — 2026-11-05

### Fixed

- A file edited inside an open folder shows its new size and date at once.

### Faster

- Filling the listing cache no longer slows down as the cache grows.
- Ctrl+K and other fuzzy lists work out highlights only for the rows on screen.
- Sorting a table viewer by the same column again does not reread every cell. Sort data stays under 32 MiB.
- PDF search keeps mostly-Latin page text in half the memory and searches more of a large PDF.

## Unreleased

### New

- The app is now called Navo. It has a new n_ icon and a start-up splash.
- The splash shows only when start-up takes longer than half a second.
- Your Ferrite profile moves to Navo by itself on first start. Settings, tabs and history come with it. The old folder is renamed to Ferrite.migrated-to-navo.
- An installed Ferrite Index service is upgraded to Navo Index. A one-time notice offers an Upgrade button.
- Workspaces: Alt+1 to Alt+9 switch between up to nine named workspaces. Holding Alt shows a tray with a card for each one. Alt+N makes a new one from the current layout.
- Each workspace keeps its own layout, target pane and tabs, also after a restart. Unsaved changes stay as a draft. Ctrl+S saves it, or Discard drops it.
- You can rename, move and delete workspaces from the tray. Ctrl+Z restores a deleted one. A workspace button in the status bar keeps the tray open.
- Tiled panes: up to four panes. Ctrl+L opens a layout switcher with five built-in layouts and room for your own, set in navo.toml or saved with Ctrl+S.
- In the layout switcher, V and S split, X closes, Shift+arrows swap, Ctrl+arrows resize and = evens the splits.
- Ctrl+Shift+1 to 4 focus a pane. Tab switches to the target pane. The target has a dashed outline for F5 and F6.
- Shift+drag on a pane header swaps two panes. Dragging a divider shows a guide line. Double-click a divider to even it. Ctrl+Alt+0 resets the split.
- Narrow windows show one pane with a strip of pane buttons. Editing the path moved from Ctrl+L to Alt+D.
- Clipboard history: Navo records the text, links and file lists you copy on this PC. Ctrl+Shift+V opens a palette in the window. Win+Alt+V opens it over any app, and Enter pastes back into that app.
- Search the history, copy an entry again, or delete one. Ctrl+K has commands to pause the history, clear it or delete all clipboard data. Settings › Clipboard turns it off.
- Copies that password managers or Windows mark as private are never recorded. The history keeps the newest 500 entries and stays on this PC.
- Git: folders that are Git repositories show a status badge, and Alt+G or a click opens a menu for your Git tool (Commit, Pull, Push, Log, Diff). With the Git command line, Pull is fast-forward only, Push is never forced, and both ask first. Settings → Git picks the tool. Not yet tried on a real Windows desktop.
- Saved items: Ctrl+N in the clipboard palette makes a snippet or a path item. Ctrl+S on a history row saves that entry as one.
- Secrets: a secret is stored encrypted with Windows user protection. It shows masked and is copied without showing. It never enters the history. It leaves the clipboard after 30 seconds, or when Navo quits.
- Folder notes: a note belongs to a folder, and there is also a global note. Notes save as you type.
- The manage view (F2) lists and edits history, snippets, paths, secrets and notes in one place.
- The Win+Alt+V pop-up now takes keyboard focus, also over other apps.
- Clear history and Delete all report only what they really removed. Delete all also removes the set-aside copies of a damaged database.
- Paste now works for files copied in a Remote Desktop session, Outlook attachments and mails, Explorer zip folders and older programs. Navo copies them into the folder like any other paste, asks about names already there before it reads anything, and lists names it cannot accept. Files from mail get the same "from the internet" mark that Explorer gives them.

### Fixed

- After a crash, Navo puts a file back that an interrupted copy, move or save had set aside, and it now writes that step to disk before it forgets the undo record. A power loss at that moment can no longer leave the original stranded.

### Backend branch

- Startup preserves damaged databases, recovers verified saved data and operations, and keeps a recovery notice with access to preserved files. Native Windows verification remains pending.
- Added the retention backend. Activation for deletes, overwrites and Recently deleted comes next.

## alpha-2026-10-26 — 2026-10-26

### New

- You can drag files from Navo into Explorer and other apps, and from them into Navo.
- Drop with the right mouse button to choose copy, move or shortcut.
- The file list scrolls by itself when you drag near its top or bottom edge.
- Folders, tabs and drives open when you hold a drag over them.
- Multi-rename names can be edited, loaded from a UTF-8 file, pasted and copied.
- Preview rows can be moved or sorted. Counters can restart in each folder.
- An optional rename log records finished batches.
- Rename works on FAT and exFAT drives, such as USB sticks.
- Rich ELF and MAP views find the memory companion file when only one fits. They apply module prefixes at once.

### Fixed

- Undo and crash recovery after a rename put back exactly the files that were renamed. A file that replaced one of them is left alone.
- On FAT and exFAT drives, undo acts only when the file size, time and kind still match. Otherwise it leaves the file and says why.
- A failed rename log never stops undo from working.

### Known problems

- Linux external drops are Copy-only.

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

- Dragging files from Navo into other apps (VS Code, Explorer) does not work yet. It comes next.

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
- Enter renames exactly what the preview shows. If the folder changed, Navo checks again first.
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
- Drag and drop with Explorer: Navo can take files dropped from Explorer. It can drag files out to Explorer. Explorer never deletes what Navo moves. The file list does not use this yet.
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

- Drag and drop: if the list changes during a drag, Navo refuses the drop. It says "The list changed during the drag. Nothing was moved." Before, a drop could move a different file or go into another folder.
- Drag and drop: on Windows, the badge treats upper and lower case as the same, as the copy does.
- Ctrl+K: key hints no longer sit under the scrollbar. Hints show Ctrl+V and Alt+Left.
- A security update to a network library.

### Built but not switched on yet

- Multi-rename: the pattern, find and replace and case fields, and the preview table with its summary are in. The window that holds them comes next.
- Rich ELF and MAP view: used flash now counts initialised data copies. TASKING ROM copies count once, under their module. A chip mapped twice gets numbered names. The screens come next.
- File index: on Windows, Navo reads NTFS volumes and their change journal directly. This needs admin. A folder keeps its contents when it is renamed between two journal reads. It is not connected to Ctrl+K yet.

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
- A newer session file is never overwritten by an older Navo.
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

- Crash report: if Navo crashes, it keeps a redacted crash record. The record holds no user names, paths or server names.
- At the next start, a notice says "Navo closed unexpectedly last time." It has Save report and Dismiss. Esc dismisses it.
- Ctrl+Shift+R moves focus to the notice.
- Save report writes a zip file whose name starts with navo-report- into Downloads, and opens a tab on it. Nothing is sent anywhere.
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

- Only one Navo runs per profile. Starting Navo again brings the running window to the front.
- `navo <path>` opens a folder in a new tab in the active pane. It opens a file in a tab on its folder, with the cursor on the file.
- A missing path shows a "was not found" message. Navo then starts as usual.
- With Navo already running, the path opens in the running window.
- If the running Navo does not answer within 2 s, the second start prints "navo: the running Navo did not answer" and exits.
- Runs with --profile or --metrics stay separate from your normal Navo.

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
- navo.toml is safer to edit while Navo runs. If you delete the file and then save from the settings sheet, Navo writes a complete new file.
- An interrupted write is recovered at the next start.
- A hand edit to a setting is never overwritten by a retry.
- New command-line option `--left <folder>` and `--right <folder>` open the panes in that folder. A missing folder is refused with a message.
- New command-line option `--profile <folder>` runs with its own settings and cache, kept in that folder.
- New command-line option `--metrics <file.json>` writes start-up time and memory to a file, then exits.

### Fixed

- Comments at the end of navo.toml stay under their own section. This fixes the known issue in the previous build.

## alpha-2026-10-17c — 2026-10-17

### New

- Shortcuts page in Settings (Ctrl+, then Shortcuts). Search by name or key. Press Enter to record a new key. Conflicts show before you save, including across nested areas, such as F10. Ctrl+R resets a row. Your comments in keymap.toml stay.
- Your own keys always win over built-in keys in the same place.
- navo.toml lists every setting. Missing settings are added at start with their default. Values you set never change. Defaults you never touched follow future versions.
- navo.toml reloads live when you edit it by hand. A mistake shows its line and column. The last good settings keep working.
- Preview on a folder stays live while files change inside it. Totals update a few times per second. "Counted at" shows when they were counted. Ctrl+R recounts.
- F3 on a folder opens the preview. Esc closes it.

### Known problems

- When Navo adds a missing section to navo.toml, comments at the very end can move under the new section. A backup copy of the file from before the session is kept next to it. The fix comes in the next build.

## alpha-2026-10-17b — 2026-10-17

### New

- Your own shortcuts: put a keymap.toml next to navo.toml, with only the keys you change. It applies when you save, with no restart.
- `keys = []` removes a shortcut.
- A key that another command uses moves to your command. The status line says which command lost it.
- A mistake in keymap.toml shows in the status line, with its line number. The working shortcuts keep working. Navo never rewrites the file.
- The "Delete for good" warning on the F-key bar follows whatever key you give to delete.

## alpha-2026-10-17a — 2026-10-17

### New

- Settings sheet (Ctrl+,): each setting applies at once, or says it is not applied yet. Undo is available. F3 jumps to the matching line in navo.toml.
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

- The Windows menu refuses names that Windows would rewrite: a trailing dot or space, or a device name such as NUL. A Windows action can then never hit another file. Navo's own actions still work there.
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
- Navo's own actions come first, then the Windows items, such as Open with, 7-Zip and Send to. Type to filter the menu.
- The context menu opens in one piece, with a short motion. A long press on an unmarked file marks it first.
- The app menu opens with F10 or the "Navo ▾" button. It has Theme, Density and Hidden files tiles. They apply at once and are saved.
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
- The app button reads "Navo ▾". The search box is centred. One drive pill has a dropdown.
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
- Marks, bookmarks and search history are remembered after you close Navo.
- Hostile files, such as huge or deeply nested images, show a message instead of freezing or crashing.

## alpha-2026-10-15e — 2026-10-15

### New

- Text files open in their real encoding. Navo checks the byte order mark, then an XML or HTML declaration, then UTF-8, then guesses, then uses the Windows ANSI code page.
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
