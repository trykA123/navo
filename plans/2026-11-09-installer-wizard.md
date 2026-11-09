# Navo installer wizard, part 1: the install engine

Status: drafting (revised after three Terra plan checks and W0, 2026-11-09) · Repo: `homelab/navo` · Branch: `installer-wizard` · Started: 2026-11-09

Architect: Turgon. Sources: the owner's choice of 2026-11-04 (memory "installer wizard"); D12, D44, D73, D88, D89, D106 and D108; roadmap v1.0 item 2; IX8 and IX8v in the [beta plan](2026-10-27-beta.md). Part 2, [the wizard UI, release and VM run](2026-11-09-installer-wizard-ui.md), builds on this part. This file holds every decision both parts use.

## Goal

Today Navo installs through Velopack's `NavoApp-win-Setup.exe`. It is one click with no window and no choice, into `%LocalAppData%\NavoApp`. After both parts, the release offers `Navo-Setup.exe`, a wizard with five pages: Welcome, Licence, Install folder, Shortcuts, Finish. The user picks and sees where Navo lives. The default is `%LocalAppData%\Programs\Navo`, and no admin is needed. Then Navo starts. Updates, uninstall and the index service keep working from the chosen folder.

Part 1 builds the engine: folder safety, install discovery, the run of Setup.exe, shortcuts, and the `--silent` mode. It runs only in dev and VM builds. No release builds or ships it (see Staging).

## What the user can do (human spec, both parts)

- **Welcome.** "Install Navo 0.4.0". If Navo is already installed, the page says "Navo 0.3.0 is installed in `<path>`. Setup updates it there. Your settings and shortcuts stay." An update keeps its folder: the Folder page shows it locked, and the Shortcuts page is skipped. While Navo is installed, Setup offers no other folder. If the same version is installed, the page offers Open Navo and Close. If a newer version is installed, the page says so and offers only Close. A damaged or unclear earlier install shows "Repair by uninstalling first: remove Navo in Windows Settings → Apps, then run Setup again." Setup does not touch that folder.
- **Licence.** The licence text scrolls. "I accept the licence" must be ticked before Next. The licence is MIT OR Apache-2.0 (P2); the page shows the MIT text and links the Apache text.
- **Install folder.** The field shows the folder Setup will create, with Browse…, the space needed and the free space. Browse picks a parent folder; Setup always installs into a new `Navo` folder inside it. A line under the field reads: "Setup creates this folder. Uninstalling Navo deletes it with everything inside, so keep your own files elsewhere." A refused folder gets one line that says what is wrong and what to do. A folder whose path holds `%`, `^`, `&` or `!` is refused, and the line asks for another folder.
- **Shortcuts.** Two check boxes, both ticked: Start menu and Desktop. An update skips this page.
- **Installing.** "Installing Navo in `<path>`…" with the elapsed time. After 3 minutes the text reads "Setup is still working…" and names the log. Back and Close stay off until Velopack's Setup.exe or `Update.exe` has exited. If Navo runs from that folder, the page says "Close Navo to continue" with Try again. Setup never closes Navo.
- **Finish.** "Navo is installed in `<path>`." An Open folder link, and "Start Navo" ticked. A line reads "Remove Navo from Windows Settings → Apps."
- **Failed.** The problem in plain words, the log path, Copy details and Back. If the failed install left files, the page names the folder and says "Delete this folder before you run Setup again."
- **Started as administrator.** "Start Setup normally, without Run as administrator." Close only.
- **Keys.**
  - Enter presses the primary button.
  - Esc closes the wizard before the install starts; nothing has changed by then.
  - Alt+B goes Back and Alt+N goes Next; Alt shows the access keys.
  - Tab cycles through the wizard, and Space toggles a check box.
  - Focus starts on the primary button and is always visible.
- **Scripted install.** `Navo-Setup.exe --silent [--installto <new folder>] [--no-desktop] [--no-start-menu]` uses the same rules without a window and exits with a code from IW17. While Navo is installed, `--installto` may name only that folder.

## Appetite

Seven agent days for both parts, VM runs and review included. Part 1 gets four of them. If the time runs out, cut in this order: Copy details, the Open folder link, the gallery pages. Never cut IW6–IW9, IW13, IW15a, IW24, or the uninstall and update VM cases.

## Goals and non-goals

**Goals:**
- The user picks and sees a folder that Setup creates and marks.
- One offline download, with no network use in the wizard.
- Updates (D89, D108) keep working from any accepted folder.
- Uninstall keeps user data and asks to remove the index service.
- An existing install upgrades in place, through Velopack's own update step (`Update.exe apply`, IW8b).

**Non-goals:**
- Program Files or per-machine installs (P1: decided no).
- Moving an install (P3: decided no; the user uninstalls and reinstalls to change the folder).
- Repairing the same version.
- An own uninstaller.
- Removing settings at uninstall.
- Installing the index service from the wizard.
- Autostart and Explorer menu entries.
- Code signing.
- ARM64, Linux and macOS.
- Protection against a process that starts after the last check (IW9).
- Protection of files the user later puts inside the install folder (IW7).
- Protection against an adversary who runs as the same user (IW0).

## Alternatives considered

- **Velopack MSI:** lost. It has no folder picker and an open per-machine bug (velopack #1040).
- **A first-run notice:** lost. The user still cannot choose.
- **Renamed Setup.exe:** lost. Nothing visible changes.
- **NSIS or Inno Setup in front of Setup.exe:** lost. It adds a second toolchain, and its look is not Navo's. D73 was already superseded by D88.
- **A download bootstrapper:** lost. It fails offline and behind proxies, and it reopens D89 (7) to save about 35 MB.
- **A zip holding the wizard and Setup.exe:** lost. The user must unzip it first.
- **`navo.exe --setup`:** lost. It ships `navo.exe` twice and runs Navo's start-up path before an install exists.
- **Setup.exe over the existing root for upgrades:** lost (R2 fired, W0 Q2). It kills Navo without a prompt, renames the root away, deletes the marker and `packages\`, re-creates deleted shortcuts and downgrades silently. With IW7a's hold it fails. Upgrades stage the nupkg and call `Update.exe apply` instead (IW8b).

## Velopack: what is proven and what is not

Proven by reading `velopack-1.2.161/src/` (the library only):

- The installed app finds its root from its own exe: the search for `\current\` is case-sensitive, and an `Update.exe` beside the exe is also accepted (`locator.rs:398-460`, `366-377`).
- Packages go to `<root>\packages` when the root is writable. Otherwise they go to `%LocalAppData%\<packId>` (`locator.rs:131-185`).
- Apply passes `--root` and `--packageDir` to `Update.exe` (`manager.rs:615-680`).
- The app's install and uninstall callbacks are dispatched in `app.rs:120-140`.
- Navo's own root detection (`navo-store/src/dirs/mod.rs:148-170`) compares `current` case-insensitively. Both agree for the layout Setup.exe writes.

W0 ([research note](2026-11-09-installer-w0-velopack.md)) read Velopack's bins at the 1.2.161 tag. Proven by that code:

- Setup.exe takes `--installto`, `--silent` and `--log`, creates missing parents, and ignores unknown flags (Q1, Q10). With `--silent` it never starts the app (Q3). It exits only 0 or 1, and 0 does not prove an install (Q9).
- Setup.exe over a root that is not empty kills every process under it, renames it away, installs fresh and deletes the old tree with the marker. With `--silent` it also downgrades (Q2, Q11). A failed fresh install is not rolled back (Q11).
- `Update.exe apply` replaces only `current\`. It keeps other root files and existing shortcuts, runs the obsolete and updated hooks (15 s each), and force-stops processes under the root. It asks for UAC even with `--silent` when the root is not writable (Q6, Q8, Q12, Q14).
- The library, not apply, replaces `Update.exe`. After a download it extracts the package entry ending in `Squirrel.exe` over `<root>\Update.exe` (`velopack` crate `manager.rs:463-475`). It then calls `Update.exe apply --package <root>\packages\<nupkg> … --root <root> --packageDir <root>\packages` (`manager.rs:615-670`).
- One HKCU key per pack id, `Uninstall\NavoApp`, with `InstallLocation` as passed (Q4). A root the user cannot write sends `packages\` and `Update.exe` to `%LocalAppData%\NavoApp` (Q13).
- Uninstall deletes the root recursively, last through `cmd.exe /C … rmdir /s /q "<root>"`, and every `.lnk` that points under the root (Q6, Q7).

The orchestrator rewrote IW3, IW4, IW7, IW8, IW11 and IW12 from W0 on 2026-11-09 (rows 52–63). W1 runs both Velopack paths in the VM. A W1 stop rule that fires changes the plan again before W3.

## Decisions

Ids IW0–IW24. Part 2's W-docs atom records them as D113. Workers never reopen a decision; a worker that finds one wrong stops and reports.

- **IW0 — Threat model (accepted, orchestrator, 2026-11-09).**
  - In scope: accidents. A wrong or existing folder; protected and data folders; network, OneDrive and `subst` paths; a running Navo; a partial failure; and a cleanup that would remove anything Setup did not create.
  - Out of scope: an adversary who runs as the same user and races renames, junctions or `subst` during the run. The wizard is never elevated (IW13), so it grants such an adversary nothing the adversary does not already have.
  - Consequence: Setup checks, holds what it creates, and rechecks after the run. It fails closed on any change it sees. It does not try to win races against the user's own processes.
- **IW1 — Velopack stays the engine.** `velopack` and `vpk` stay at =1.2.161, with pack id `NavoApp`, the channel and the feed as they are (D89). The wizard runs Velopack's own Setup.exe and `Update.exe` and never forks them. For an upgrade it puts the package's own `Update.exe` in place, as the library does (IW8b).
- **IW2 — Default parent `FOLDERID_UserProgramFiles`** (`%LocalAppData%\Programs`). The default root is `<that>\Navo`.
- **IW3 — The two Velopack calls**, each an argument array with no shell:
  - Fresh: `<work>\Setup.exe --installto <root> --silent --log <velopack log>` (W0 Q1, Q10).
  - Upgrade: `<root>\Update.exe apply --package <root>\packages\<nupkg> --silent --norestart --root <root> --packageDir <root>\packages --log <velopack log>`. This is the library's own call (`manager.rs:615-670`) plus `--log`; `--norestart` keeps Navo from starting.
  - Velopack ignores unknown flags (W0 Q1), so a typo would pass unseen. W8b and W8u test both arrays exactly.
  - `<velopack log>` is `<run stem>-velopack.log` beside the wizard's own log (IW15a). It is a separate file, so two writers never share one. Velopack may roll it to `.old` above 1 MB (W0 Q10).
- **IW4 — One child process, waited to the end.**
  - At most one Velopack process (Setup.exe or `Update.exe apply`) runs at a time.
  - The wizard waits until it exits. A deadline of 180 s changes only the text; the process is never killed.
  - Back, Retry and Close stay off until it exits.
  - Success needs six things, for a fresh install and an upgrade alike:
    1. exit code 0 from Setup.exe or `Update.exe apply`; 0 alone proves nothing (W0 Q9);
    2. a regular `<root>\Update.exe`; after an upgrade, with the SHA-256 of the one the wizard placed (IW8b);
    3. a `<root>\current\sq.version` with id `NavoApp` and the payload's version (IW15);
    4. the marker (IW7): a fresh install writes it only after checks 1–3, 5 and 6 pass, and succeeds once it is written; a marked upgrade keeps the `installId` discovery read; a legacy upgrade needs the legacy identity instead (IW8);
    5. the chain unchanged (IW7a): the held folders keep their identity and have no reparse tag, and the existing ancestors re-resolve to the identities seen before the run;
    6. the HKCU key `Uninstall\NavoApp` names the root: for a fresh install its `InstallLocation` is the exact `--installto` text (W0 Q4), and in both modes it resolves to the held root's identity.
- **IW5 — No Cancel during the install.** Before it, Esc and Cancel close at once.
- **IW6 — Path syntax (pure, fail closed).** A candidate is refused when any of these holds:
  - it is not absolute;
  - it has a `.` or `..` component;
  - it is a UNC, `\\?\`, `\\.\` or device path;
  - it has a `:` beyond the drive letter (alternate data stream);
  - it has a character Windows rejects, a reserved name (`CON`, `NUL`, `COM1`…), or a component that ends in a dot or a space;
  - it is longer than 150 UTF-16 units;
  - it is a drive root;
  - it has `%`, `^`, `&` or `!` in any component. Uninstall passes the root to `cmd.exe /C … rmdir /s /q "<root>"` as raw text, where `%NAME%` expands even inside quotes, and `!NAME!` too when delayed expansion is on (W0 Q7). A root such as `C:\%HOMEPATH%` would make uninstall delete `C:\Users\<u>`. `^` and `&` are refused as well (orchestrator, 2026-11-09). The problem text names the character. For the default folder, it says the user folder's name holds it and asks for another folder, such as `C:\Navo`.
  - Discovered roots get the same check (IW8).
- **IW7 — Setup owns a new folder.**
  - The root must not exist. Browse returns `<picked>\Navo`; a typed path is used as typed.
  - Every existing ancestor is resolved by file identity (volume serial plus 128-bit file id). Any reparse point (junction, symlink, OneDrive placeholder) anywhere in the chain is refused.
  - The chain may not pass through a refused folder (IW8a list), compared by identity, never by text. Short names, letter case, `subst` drives and aliases therefore cannot slip past.
  - The drive must be local: `GetDriveTypeW` is fixed or removable.
  - The drive must not be a `subst` or other DOS-device alias. `QueryDosDeviceW("X:")` must succeed, and its target must not start with `\??\`. Otherwise the folder is refused (fail closed).
  - A probe file in the nearest existing ancestor must be created and deleted without elevation: `CREATE_NEW` with `FILE_FLAG_DELETE_ON_CLOSE`, share none, closed at once, so Setup's own handle deletes it.
  - The free space must be at least the larger of 4 × the payload size and Velopack Setup.exe's own requirement, computed the same way (W1 run 2: Setup.exe asked for 232 MB where 4 × payload was about 188 MB; W4b reads its formula in `velopack/src/bins/src/` and cites it). A folder that passes the wizard must never fail Setup.exe on space.
  - The wizard then creates and holds the missing folders and the root (IW7a). It re-resolves the chain and the refused set (IW8a) and refuses if anything changed.
  - The root is empty when Setup.exe starts: the wizard creates it and writes nothing into it. Setup.exe treats a root that is not empty as an earlier install and renames it away (W0 Q2, Q11).
  - After Setup.exe exits 0 and IW4's checks 1–3, 5 and 6 pass, the wizard writes the marker `<root>\navo-install.json` = `{"packId":"NavoApp","installId":"<32 hex from a CSPRNG>","createdBy":"navo-setup <version>"}`. It writes with `CREATE_NEW` through the still-held root, flushes and closes the handle, and records the marker's file id. The install counts as a success only once the marker is written (IW4 check 4).
  - A failed Setup.exe can leave partial files in the root, because Velopack does not roll back a fresh install (W0 Q11). IW7a's cleanup removes only the empty folders this session created. A root that is not empty is kept: the Failed page names it and says "Delete this folder before you run Setup again." Silent mode prints the same line and exits 5. A Setup.exe that wrote nothing therefore leaves nothing (S10 `fail-empty`).
  - Recovery after Setup.exe exits 0. Setup never deletes installed files. Silent mode exits 5 with the same text.
    - Marker not written (the write failed): the Failed page says "Navo is installed in `<path>`, but Setup could not mark the folder." Retry calls `mark_again`, which writes the marker with `CREATE_NEW` into the held root. The Failed page keeps the session, and so its handles, until Retry succeeds or the user closes.
    - A file already at the marker path, which the wizard did not write: `CREATE_NEW` fails, and the file is never overwritten. The page says "Navo is installed in `<path>`, but its folder carries another mark. Remove Navo in Settings → Apps, then run Setup again."
    - No HKCU entry names the root: the page says "Navo is installed in `<path>`, but Windows does not list it. To remove it, run `"<path>\Update.exe" uninstall`, or delete the folder." Copy details copies the command; W0 Q4 confirms its form.
    - If the wizard closes with the marker missing, the next run finds the folder Ambiguous through its registry entry, and the user removes Navo in Settings → Apps.
  - W0 Q11 showed that a marker written first makes Setup.exe take its existing-install path, so the write moved after verification (row 52). W1 P1 and P1h prove the empty-root install.
  - Uninstall deletes the root with everything inside (W0 Q7: `remove_dir_contents`, then `rmdir /s /q`). That includes files the user adds there later. This is accepted, as with every installer, and the Folder page and the user guide say so.
  - Any probe error refuses the folder with "Setup could not check this folder".
  - After a failed Setup.exe, Retry in the same session reuses the root only while it is still empty and its held folders are unchanged; otherwise Retry is off. On Close, IW7a's cleanup removes the empty folders this session created, and a root that is not empty is kept and named, as above.
- **IW7a — Hold what Setup creates, recheck the rest (both modes).**
  - **Hold.** The wizard holds a handle to the root and to every folder it created: `CreateFileW` with access `FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE`, flags `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, share read and write. The read access makes Windows enforce the missing `FILE_SHARE_DELETE`, so no other process can rename or delete a held folder. The handle asks for no `DELETE`, so Setup.exe's own opens need only share read.
  - **Fresh.** Each missing folder is created with `CreateDirectoryW` (fails if it exists) and opened as above at once. A reparse tag on it refuses. That the opened folder is the one just created is not proven; that race is out of scope (IW0).
  - **Upgrade.** The existing root is opened the same way. Its identity must equal the one discovery saw (IW8).
  - **Recheck.** Existing ancestors are not held. Their identities and reparse flags are read at the check, again just before spawn, and again after the Velopack process exits. The held folders are re-read through their handles after the exit. Any change fails closed: before spawn exit 3, after the exit the Failed page with the files kept.
  - The handles stay open through the child run, verification and any Failed page. They close at cleanup or Close.
  - **Cleanup** deletes only the folders this session created, deepest first, from the root up. It never deletes a file: the marker exists only after a success, and a success deletes nothing. For each folder:
    1. close its hold handle;
    2. reopen it by path with `DELETE | FILE_READ_ATTRIBUTES` and `FILE_FLAG_OPEN_REPARSE_POINT`;
    3. check that the file id equals the recorded one and that there is no reparse tag;
    4. set `FileDispositionInfo` through that handle;
    5. close it, which completes the delete, and only then go to the parent.
    A folder that is not empty, or whose id differs, is kept, and so are all its parents. Close on a recovery page (IW7) only closes handles and deletes nothing.
  - **Why ancestors are rechecked, not held.** Holding `C:\Users\<u>` and the other ancestors guards only against IW0's out-of-scope adversary. Each held folder adds a sharing conflict that Setup.exe or another program can hit. An accidental rename is caught by the recheck.
  - W1 P1h proves that a fresh Setup.exe works with these exact handles on an empty root. P3h proves that `Update.exe apply` works with the existing root held; apply renames only `current\`, which is not held (W0 Q12). If either fails, stop (R8).
- **IW8 — Existing installs, fail closed (P3 decided 2026-11-09: no move).** Discovery reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NavoApp` → `InstallLocation` (W0 Q4) and the legacy `%LocalAppData%\NavoApp`. A folder counts as *valid* when all of these hold:
  - its root passes IW6, and its chain passes IW7's identity, reparse and `subst` checks and IW8a's never-inside list;
  - it holds a regular `Update.exe`;
  - its `current\sq.version` parses to id `NavoApp` and a version;
  - either its marker has `packId` `NavoApp`, or its identity equals the legacy folder;
  - if the registry names a folder, that folder has the same identity.

  The outcomes:
  - Nothing found: Fresh. Only then may the user pick a folder. Every Velopack install of Navo owns the one key `Uninstall\NavoApp` (W0 Q4), so a fresh install in a second folder would take over the first one's entry. While any install is found (the key or the legacy folder), Setup offers no fresh install. Silent `--installto` naming another folder exits 3 with "Navo is already installed in `<path>`. Setup updates it there." The window shows the folder locked.
  - One valid install, older: Upgrade in place through `Update.exe apply` (IW8b), with the folder locked and the root held (IW7a). A legacy root gets the marker if it is missing. A failed write there is logged, and the upgrade still counts, because the legacy identity keeps the folder valid.
  - The same version: Same (Open Navo, Close).
  - A newer version: Newer (Close). This check is the only downgrade guard: Setup.exe with `--silent` downgrades without asking (W0 Q2), so the wizard starts no Velopack process in Same or Newer. W5 and W8b test it, and S8 checks that no Velopack process starts.
  - A root that fails IW6's character rule: Ambiguous with reason `UnsafeName` and its own text: "Navo is installed in `<path>`. Setup cannot update a folder whose name contains `<char>`. Keep using Navo there; it updates itself." The text never suggests uninstalling, which is the step the rule guards.
  - Anything else is **Ambiguous**: a registry entry without a valid folder, two different valid folders, a damaged or foreign `sq.version`, or a non-legacy folder without a marker. The page says "Repair by uninstalling first", and Setup touches nothing (exit 6).
- **IW8b — Upgrade through `Update.exe apply` (R2 fired; orchestrator, 2026-11-09).** Setup.exe runs only for a fresh install into an empty root. An upgrade repeats the library's own steps:
  1. Discovery found one valid older install. The root is held (IW7a), and its identity equals discovery's.
  2. Write check: a probe file in the root, as in IW7. If it fails, Setup refuses with "Setup cannot write to `<path>`" (exit 3) and asks for no UAC. Otherwise `Update.exe apply` would relaunch itself elevated even with `--silent` (W0 Q8), and Velopack would move `packages\` and `Update.exe` to `%LocalAppData%\NavoApp` (W0 Q13).
  3. The running check (IW9). A block writes nothing.
  4. The package. The wizard writes the embedded full nupkg (IW15) to `<root>\packages\<nupkg name>.navo-partial` with `CREATE_NEW`, checks its SHA-256 through that handle, then renames it to `<nupkg name>` with `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`, as the library does with its `.partial` file. It creates `packages\` if it is missing. After the rename, it deletes every other `*.nupkg` and `*.partial` in `packages\` through its own handles, as the library does after a download (`manager.rs:418-438, 478-481`; W1 P2u). It never deletes another file there. `packages\.betaId` is left to the library, which creates it at its next check.
  5. The new `Update.exe`. The wizard extracts the nupkg's single entry ending in `Squirrel.exe`, the one the library extracts (`manager.rs:463-475`), to `<root>\Update.exe.navo-new` with `CREATE_NEW`. It then replaces `<root>\Update.exe` with the same `MoveFileExW` flags and records the SHA-256 for IW4 check 2. A failed replace, for example while an `Update.exe` still runs, keeps the old file and fails with exit 5 before apply. If apply fails later, the root keeps the new `Update.exe` with the old `current\`: the same state a failed in-app update leaves.
  6. The ancestors are rechecked and the running check runs again. Then `Update.exe apply` runs with IW3's array.
  7. Apply runs the old version's `--veloapp-obsolete` and the new one's `--veloapp-updated` hook, 15 s each, then force-stops every process under the root (W0 Q14). Navo registers only the uninstall callback (`navo-app/src/installer.rs:25-33`), so both hooks exit at once. The force-stop hits only a Navo started after the last scan; IW9 accepts that race.
  8. Verify per IW4. Discovery and verification expect Velopack's own extras in a root: a top-level `Navo.exe` stub, `packages\.velopack_lock` and an empty `packages\VelopackTemp\` (W1). Apply keeps the marker and every other root file, and it never adds or re-creates a shortcut (W0 Q6, Q12).
  - Leftover `.navo-partial` and `.navo-new` files are deleted by handle, as in IW15a. Nothing else in the root is deleted after a failed upgrade.
  - W1 P2, P2u, P3 and P3h prove this path. P3s records what Setup.exe over the root does, as R2's evidence only.
- **IW8a — Refused folders.** These come from `SHGetKnownFolderPath` with `KF_FLAG_DEFAULT`, so redirected and OneDrive locations count:
  - **Never inside:** Windows, System, ProgramFiles, ProgramFilesX86, ProgramFilesCommon, ProgramData, Navo's data and config folders (`%LocalAppData%\Navo`, `%AppData%\Navo`; `navo-store/src/dirs/mod.rs:61-67`), the legacy Ferrite data and config folders (`dirs/mod.rs:40-49`), OneDrive (`FOLDERID_SkyDrive`), and Startup.
  - **Never equal** (inside is allowed, since the root is always a new child): Profile, Desktop, Documents, Downloads, Pictures, Music, Videos, LocalAppData, RoamingAppData, LocalAppDataLow, UserProgramFiles, StartMenu and Public.
  - **Fresh only, never inside:** the legacy root `%LocalAppData%\NavoApp`, which is also Velopack's fallback `packages` folder. An upgrade of the legacy root is allowed.
  - **Absent folders count.** A protected folder that does not exist is kept as its nearest existing ancestor's identity (the anchor) plus the missing names. A candidate is inside it when the candidate's chain holds the anchor and the candidate's next names match the missing names. Names match by `CompareStringOrdinal(a, b, TRUE)`: ordinal and ignoring case, the comparison Microsoft recommends for file names. So on a clean profile, before Navo ever ran, `%LocalAppData%\Navo` and `%LocalAppData%\navo\x\Navo` are refused.
  - Fail closed where that comparison may not match the file system. An anchor that is a case-sensitive directory (`FileCaseSensitiveInfo`), or whose case sensitivity cannot be read, refuses every candidate whose chain holds it. Trailing dots and spaces never reach the comparison (IW6). Short names exist only for existing folders, and those are compared by identity.
  - IW7's recheck re-resolves this set, so a protected folder created in between is caught by identity.
  - A known folder that cannot be resolved is skipped only when Windows reports `E_FAIL` or `FILE_NOT_FOUND` for it. Any other error refuses.
- **IW9 — Never close Navo, and what is guaranteed.** Before an upgrade, Setup refuses to touch a root while a process it can see runs from it. That is any process whose image path it can read under the root, or an unreadable `navo*` image in the user's own session (the updater's rule, `navo-app/src/updater/apply.rs:196-202`). A scan error counts as running. Processes of other users or sessions that it cannot read, and a launch after the scan, are not covered. This race is accepted. `Update.exe apply` force-stops what runs under the root after its hooks (W0 Q14); W1 P3 records it, and the user guide documents it. The check runs twice for an upgrade: before the wizard writes anything, and just before apply (IW8b). The scan is reused from the updater (W2a, W2b).
- **IW10 — One Apps & Features entry, Velopack's.** The wizard writes no registry values. Outside the root it writes only its private temp folder and its logs (IW15a).
- **IW11 — Shortcuts (W0 Q5, Q6).**
  - `vpk pack` keeps its default `Desktop,StartMenuRoot`; changing it would alter existing installs at their next update.
  - A fresh Setup.exe creates `FOLDERID_Desktop\Navo.lnk` and `FOLDERID_StartMenu\Programs\Navo.lnk`. For an unticked box, the wizard deletes that `.lnk` after success, only at that exact path and only when `IShellLinkW` resolves its target under the root.
  - `Update.exe apply` never adds or re-creates a shortcut; it only updates the ones that point into the root. So the choice holds after every update.
  - An upgrade skips the Shortcuts page and keeps the user's shortcuts as they are. Silent `--no-desktop` and `--no-start-menu` have no effect on an upgrade, and the log says so.
  - A Setup.exe run over a root would re-create both (W0 Q6); IW8b never does that.
- **IW12 — Start Navo** runs `<root>\current\navo.exe` detached, from the profile folder. The wizard is never elevated (IW13), so Navo is not either. W0 Q3: Setup.exe with `--silent` never starts Navo, and apply runs with `--norestart`, so the box stays. Setup.exe still runs `navo.exe --veloapp-install` as a hook; that is not a start.
- **IW13 — No elevation, fail closed.** The exe carries a manifest with `requestedExecutionLevel level="asInvoker"`. Without it, Windows' installer detection would elevate any `*Setup*` exe. The manifest also sets per-monitor-v2 DPI awareness.
  - At start, `GetTokenInformation(TokenElevation)` is read. An elevated token gets the "Started as administrator" page (silent: exit 7). If the call fails, the result is the same.
  - Consequence: an admin account with UAC turned off cannot use the wizard. The user guide points it to the portable zip.
- **IW14 — Own exe.**
  - Crate `navo-setup`, binary `navo-setup`, released as `Navo-Setup.exe`.
  - Direct dependencies: `navo-ui`, `navo-l10n`, `navo-core`, `navo-diag`, `navo-store` (data and config folders), `navo-update` (`Version`, `parse_version`, semver ordering with prereleases), `serde` 1.0.229 (derive), `serde_json` 1.0.154, `sha2` =0.10.9, `roxmltree` =0.21.1 and `zip` 9.0.3 (default features off, `deflate`, as in `navo-app`; it reads the nupkg, IW8b). On Windows: `windows-sys` 0.61.2, and `windows` =0.62.2 for COM (`IShellLinkW`, the folder dialog). Build dependencies: `embed-resource` 3.0.12 and `sha2` =0.10.9. All are already in `Cargo.lock`.
  - The ban is on direct dependencies and code paths. `navo-setup` never lists `navo-app`, `navo-index`, `navo-index-service`, `velopack` or an HTTP client. `navo-ui` brings `navo-index` and gpui's HTTP client in transitively; that is accepted. No code in `navo-setup` calls them, and the wizard opens no connection (S11). `velopack` must not appear anywhere in its tree.
  - Its version resource is "Navo Setup", built from `navo-app/build_resource.rs`.
- **IW15 — Embedded payload.**
  - `build.rs` copies `NAVO_SETUP_PAYLOAD` (a Setup.exe) and `NAVO_SETUP_PACKAGE` (the full nupkg of the same version) into `OUT_DIR`. It emits their SHA-256s and sizes and the nupkg's file name. Fresh installs use the Setup.exe, upgrades the nupkg (IW8b). This adds about one Setup.exe to the wizard's size, since the nupkg is inside Setup.exe too; the wizard cannot reuse that copy without parsing Velopack's private bundle format.
  - The exe embeds it with `include_bytes!`, so a later Authenticode signature covers it.
  - `NAVO_SETUP_REQUIRE_PAYLOAD=1` makes a missing payload or package a build error. Without both, the wizard shows "This build carries no Navo package" (exit 9).
  - The payload carries its version. An embedded payload has `CARGO_PKG_VERSION`; the release checks that it equals the pack version (part 2, W14).
  - At start, the wizard reads the nupkg's `.nuspec` with `parse_sq_version` (`sq.version` is the same format). Its id must be `NavoApp` and its version the payload's; otherwise exit 9 with "This build carries a damaged Navo package".
  - The `tester` feature adds `--payload <Setup.exe> --payload-package <nupkg> --payload-version <semver>`, all three or none (exit 2 otherwise).
  - IW4, IW8 and the Welcome text use the payload's version, never `CARGO_PKG_VERSION` directly.
- **IW15a — Temp and log handling.**
  - The work folder is `%TEMP%\NavoSetup-<32 hex>`, created with `CreateDirectoryW` and a DACL that grants only the user's SID and SYSTEM, with no inheritance.
  - `Setup.exe` is written through a `CREATE_NEW` handle, which is closed and reopened read-only with share mode read only. The hash is checked through that handle, and the handle stays open until Setup.exe exits, so nothing can rewrite or rename the file.
  - Cleanup deletes `Setup.exe` and then the work folder as IW7a's cleanup does: close the handle, reopen with `DELETE` and `FILE_FLAG_OPEN_REPARSE_POINT`, check the recorded file id, set the disposition, close.
  - The hash detects a damaged extraction. It does not authenticate anything: an unsigned wizard that was replaced is replaced as a whole. The docs say this plainly.
  - Logs go to `%TEMP%\NavoSetup-logs\<UTC time>.log`, and Velopack's log for the same run to `<UTC time>-velopack.log` beside it (IW3). The last 5 runs are kept, each with its Velopack log and any `.old`. Older logs are deleted the same way, by handle, and a reparse point among them is skipped. Cleanup never deletes logs.
- **IW16 — Release assets** are in part 2. Nothing is published before part 2's VM run and review.
- **IW17 — Exit codes for `--silent`.**
  - 0 installed or upgraded
  - 2 bad arguments
  - 3 folder refused, including a root Setup cannot write and a second folder while Navo is installed
  - 4 Navo running
  - 5 Setup.exe or `Update.exe apply` failed (its code in the message), or a fresh install left files in the root
  - 6 existing install ambiguous
  - 7 elevated, or elevation unknown
  - 8 same or newer installed
  - 9 no payload

  Messages go to stderr. Silent mode never starts Navo and waits until the Velopack process exits.
- **IW18 — Per-user only (P1 decided 2026-11-09: yes).** This follows from IW13: refusing elevation rules out Program Files. No atom waits on it.
- **IW19 — Licence page (P2 decided 2026-11-09: MIT OR Apache-2.0, keep the box)**, built in part 2.
- **IW20 — Uninstall keeps user data.** Velopack removes the root and the shortcuts. The existing hook (`navo-app/src/installer.rs:22-31,68-90`) asks for UAC to remove the index service, only when a service exists, within 25 s. The removal is not guaranteed: a declined prompt or the 25 s limit leaves the service in place and logged. The user guide gives `sc.exe delete "Navo Index"` and the `Program Files\Navo` folder for that case.
  - Uninstall also deletes every `.lnk` whose target is under the root: on the Desktop, in Startup, in the Start menu and among pinned items, including shortcuts the user made (W0 Q6). The user guide says so (part 2's W17). No code.
- **IW21–IW23 — Look, keys and strings** are in part 2.
- **IW24 — DLL planting defence.** `Navo-Setup.exe` runs from Downloads, where a planted DLL may sit beside it.
  - The static CRT is already set (`.cargo/config.toml:5`).
  - `build.rs` links with `/DEPENDENTLOADFLAG:0x800` (search System32 only for the image's dependencies) and with `/DELAYLOAD:<dll>` for every import that is not a KnownDLL.
  - The first statement in `main` is `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`.
  - `scripts/setup-imports.ps1` fails when `dumpbin /imports` lists a non-delayed import outside `HKLM\…\Session Manager\KnownDLLs`.
  - The extracted Setup.exe runs from the private work folder, where nothing can be planted.
  - VM case S13 (part 2) plants DLLs.
- **Staging.** Part 1 builds in every gate, because the workspace members glob is `crates/*`. The `navo-setup` binary therefore exists in dev and VM builds. Without a payload it installs nothing (exit 9). The payload comes only from `NAVO_SETUP_PAYLOAD`, or `--payload` in `tester` builds. `release.yml` builds only `-p navo-app -p navo-index-service` until part 2 changes it. W2b is reachable: it changes the live updater's code path. It is a pure move with the same tests and lands only with VM CI green.

## Risks and rollback

- **R1 — Data loss through uninstall.** Velopack deletes the root. Prevented by IW6–IW8b, including IW6's character rule for the `cmd` call: Setup only ever installs into a folder it created, and an ambiguous earlier install is refused. Files the user later puts inside are lost by design (IW7). Tests: `rules` and `placement` tests (with a clean profile), W7d, S4 and S6.
- **R2 — Setup.exe over an existing root misbehaves. Fired (W0 Q2):** it kills Navo, renames the root, deletes the marker and `packages\`, re-creates shortcuts and downgrades silently; with the hold it fails. IW8b upgrades through `Update.exe apply` instead. W1 P3s records Setup.exe's behaviour as evidence only.
- **R3 — Velopack was only read.** W1 runs it before any builder starts.
- **R4 — Shortcut drift.** W0 Q6 shows that apply never re-creates a link, and IW11 follows it. W1 P4 and part 2's S9 check it.
- **R5 — The index service stays after a declined UAC prompt.** This is existing behaviour, documented (IW20).
- **R6 — A missed DLL in IW24** gets loaded from Downloads. The imports check and S13 cover it. Rollback: none needed before part 2 publishes.
- **R7 — An admin with UAC off cannot install** (IW13). Accepted; the portable zip is the way.
- **R8 — Velopack fails with IW7a's handles open.** Fresh: Setup.exe into the empty held root (W1 P1h). Upgrade: `Update.exe apply` with the root held (W1 P3h); apply renames only `current\` (W0 Q12). Both use the exact production flags. If either fails, stop; the answer is a plan change, never a silent drop of the hold.
- **R9 — The wizard's apply steps differ from the library's.** IW8b repeats three library steps by hand: the package into `packages\`, the cleanup of older packages, and `Squirrel.exe` over `Update.exe`. W1 P2u compares the result with Navo's own update, and part 2's S2b and S3 run it end to end. The compared state is: the `*.nupkg` set in `packages\`, `Update.exe`'s SHA-256, `sq.version`, the marker and the HKCU values. Not compared: `.betaId` (the library makes it lazily), `.velopack_lock`, an empty `VelopackTemp\`, and `Update.exe`'s file id (the library writes in place, the wizard renames; the bytes are the same). The `Update.exe` hash cannot catch a wrong step, because every pack from one vpk carries identical bytes; the `.nuspec` check in IW15 is the guard there. If the compared state differs, stop. W1 run 1 (2026-11-09) found the missing package cleanup; fixed above.
- Rollback for part 1: `git revert` of each atom. W2b alone touches shipped code, and it is a pure move.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `process-scan` | platform, `navo-diag` | `processes::any_under(root: &Path) -> io::Result<bool>`; off Windows `Err(Unsupported)` | moved from `navo-app/src/updater/apply.rs:196-360` | `navo-diag/src/processes/tests.rs` |
| `setup-crate` | bin crate | `crates/navo-setup`; `tester::dispatch(&[OsString]) -> Option<ExitCode>` | new | `cargo xtask check -p navo-setup` |
| `path-syntax` | core, `syntax.rs` | `parse_candidate(&OsStr) -> Result<Candidate, FolderProblem>` | new | `syntax_tests.rs` with proptest |
| `placement` | core, `placement.rs` | `check(&Chain, &Refused, &Facts, payload: u64) -> Result<(), FolderProblem>`; `inside(&Chain, &Protected, &dyn NameEq) -> bool`; `browse_target(&Path) -> PathBuf` | new | `placement_tests.rs` |
| `install-mode` | core, `mode.rs`, `marker.rs` | `parse_sq_version(&str) -> Result<SqVersion, ModeError>`; `Marker::{parse, render}`; `mode(&Discovery, own: &Version) -> Mode { Fresh, Upgrade, Same, Newer, Ambiguous(Why) }` | new | `mode_tests.rs` with proptest |
| `setup-flow` | core, `flow.rs` | `step(Flow, Event) -> (Flow, Vec<Effect>)` | new | `flow_tests.rs` |
| `chain-probe` | platform, `probe/chain.rs` | `resolve_chain(&Candidate) -> io::Result<Chain>`; `refused() -> io::Result<Refused>` | new | VM via `--tester-probe` |
| `facts-probe` | platform, `probe/facts.rs` | `facts(&Chain) -> io::Result<Facts>`; `elevated() -> io::Result<bool>` | new | VM via `--tester-probe` |
| `discovery` | platform, `probe/discovery.rs` | `discover() -> io::Result<Discovery>` | new | VM S8, S9, S14 |
| `chain-pin` | platform, `probe/pin.rs` | `Pin::fresh(&Chain)`, `Pin::existing(&Path, FileId)` → `io::Result<Pin>`; `unchanged()`, `ancestors_unchanged(&Chain)` → `io::Result<bool>`; `remove_created(self) -> io::Result<Removed>` | new | `pin_tests.rs` (Windows); VM T5 |
| `payload` | platform, `payload.rs`, `build.rs` | `Payload::embedded()` with the Setup.exe, the nupkg and its checked `.nuspec`; `extract(&Work) -> io::Result<Extracted>` (handle held) | new | `payload_tests.rs` |
| `upgrade-prep` | platform, `upgrade.rs` | `writable(&Pin) -> io::Result<bool>`; `stage_package(&Pin, &Payload) -> io::Result<Staged>`; `replace_update_exe(&Pin, &Payload) -> io::Result<[u8; 32]>`; `apply_args(&Root, &Staged, log: &Path) -> Vec<OsString>` | new | `upgrade_tests.rs` (Windows) |
| `dll-guard` | build, `build.rs`, `main.rs` | link flags; first call in `main` | new | `scripts/setup-imports.ps1`; S13 |
| `setup-runner` | background, `runner.rs`, `host.rs` | `install(Request, &dyn Host) -> Result<Installed, InstallError>` (one child, waits to exit); `mark_again(&Session) -> Result<(), InstallError>` | new | `runner_tests.rs` with a fake host |
| `shortcuts` | platform, `shortcuts.rs` | `remove_unticked(Shortcuts, &Root) -> io::Result<Removed>` | new | `shortcuts_tests.rs` (Windows) |
| `setup-cli` | commands, `cli.rs` | `parse(args) -> Result<Cli, CliError>`; silent run → IW17 code | new | `cli_tests.rs` |

## Atoms

| Atom | Component | Role | Effort | After | Owner gate | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W0 | research | researcher | medium | — | — | research note answers Q1–Q14 with file:line | done (2026-11-09) |
| W1 | VM probe | checker | high | W0, ix8v merged, G1 | — | report P1–P10, P1h, P2u, P3h, P3d and P3s with evidence | done (2026-11-09) |
| W2a | `process-scan` | api-builder | medium | W1 | — | `cargo nextest run -p navo-diag processes` | blocked |
| W2b | `process-scan` (use) | patcher | medium | W2a | — | `cargo nextest run -p navo-app updater::apply`; VM CI green | blocked |
| W3 | `setup-crate` | patcher | medium | W1 | — | `cargo xtask check -p navo-setup`; `cargo tree -p navo-setup -e normal --depth 1 --prefix none --target all` lists exactly IW14's direct dependencies; `cargo tree -p navo-setup -e normal --prefix none --target all` exits 0 and has no line starting `velopack ` | blocked |
| W4a | `path-syntax` | senior | high | W3 | — | `cargo nextest run -p navo-setup syntax` | blocked |
| W4b | `placement` | senior | high | W4a | — | `cargo nextest run -p navo-setup placement` | blocked |
| W5 | `install-mode` | api-builder | high | W3 | — | `cargo nextest run -p navo-setup mode` | blocked |
| W6 | `setup-flow` | api-builder | high | W4b, W5 | — | `cargo nextest run -p navo-setup flow` | blocked |
| W7a | `chain-probe` | senior | high | W4b | — | `cargo xtask check -p navo-setup` on Windows; VM `--tester-probe chain` table T1 | blocked |
| W7b | `facts-probe` | senior | high | W7a | — | VM `--tester-probe facts` table T2 | blocked |
| W7c | `discovery` | senior | high | W5, W7b | — | VM `--tester-probe discover` table T3 | blocked |
| W7d | `chain-pin` | senior | high | W7c | — | `cargo nextest run -p navo-setup pin` (Windows); VM `--tester-probe pin` table T5 | blocked |
| W8a | `payload` | api-builder | high | W3 | — | `cargo nextest run -p navo-setup payload` (Windows) | blocked |
| W8h | `dll-guard` | senior | high | W8a | — | `scripts/setup-imports.ps1 target\release\navo-setup.exe` prints PASS | blocked |
| W8u | `upgrade-prep` | senior | high | W7d, W8a | — | `cargo nextest run -p navo-setup upgrade` (Windows) | blocked |
| W8b | `setup-runner` | senior | high | W2b merged with VM CI green, W6, W7b, W7c, W7d, W8a, W8u | — | `cargo nextest run -p navo-setup runner` | blocked |
| W8s | `shortcuts` | senior | high | W8b | — | `cargo nextest run -p navo-setup shortcuts` (Windows) | blocked |
| W8c | `setup-cli` | api-builder | medium | W8b, W8s, W8h | — | `cargo nextest run -p navo-setup cli`; VM: exit codes table T4 | blocked |

19 atoms, close to the 15-atom guide; part 2 holds the rest. **G1 (orchestrator, after ix8v merges):** re-read `scripts/update-e2e.ps1` and `NAVO_UPDATE_FEED_DIR` on main, and update the line numbers in W1 and in part 2's W15. The ix8v numbers below are from `78a73adc`. G1 done 2026-11-09: unchanged on main `91e7a4e7`. **No builder before W1.** The orchestrator rewrote IW3, IW4, IW7, IW8, IW11 and IW12 from W0 (rows 52–63); W1 confirms them first. One cargo gate at a time in the Windows VM. Each worker writes gate output to `<worktree>/../<atom>.log` and reads only its tail.

### W0 — Velopack's Setup.exe and Update.exe, from source

- **Read:** the `velopack/velopack` repo at the tag of vpk 1.2.161, cloned into a new empty scratch folder; `src/bins/src/` only.
- **Answer with file:line:**
  - Q1: Setup.exe's arguments, and whether parent folders are created.
  - Q2: Setup.exe over an existing NavoApp root (older, same, newer): hooks, kills, `packages`.
  - Q3: whether Setup.exe starts the app after `--silent`, and whether it can be switched off.
  - Q4: the HKCU key and its values.
  - Q5: the shortcut names, folders and targets, and vpk's default `--shortcuts`.
  - Q6: whether an update re-creates a shortcut the user deleted, and how uninstall finds shortcuts.
  - Q7: whether uninstall deletes the root recursively, and whether it follows reparse points.
  - Q8: whether `Update.exe apply` elevates itself for a root it cannot write.
  - Q9: Setup.exe's exit codes.
  - Q10: where the logs go.
  - Q11: whether Setup.exe accepts a pre-created empty root.
  - Q12: whether an update or apply keeps unknown files in the root (the marker).
  - Q13: whether `Update.exe` in a custom root reads or writes registry values that assume `%LocalAppData%\NavoApp`.
  - Q14: whether the uninstall callback's exit code or a timeout changes what uninstall does.
- **Stop if:** the tag cannot be matched to 1.2.161.

### W1 — VM probe of Velopack's two paths (no code)

- **Read:** the W0 note; `scripts/update-e2e.ps1` per G1 (`Build-Pack`, `Install-Base`, `Uninstall-Copy`, `Start-App`); the `velopack` crate's `manager.rs:463-475` and `615-670`.
- **Terms.** "The hold" is IW7a's exact handle, opened from a PowerShell `Add-Type` helper: `CreateFileW`, access `FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE`, flags `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, share read and write. "The apply steps" are IW8b's steps 4–6 by hand: copy the full nupkg into `<root>\packages\`, extract its `Squirrel.exe` entry over `<root>\Update.exe`, then run IW3's apply array with `--log <file>`.
- **Do** on a clean snapshot, as the standard user unless named.
  - Fresh path:
    - P1: Setup.exe 0.0.2 `--installto` a pre-created empty `C:\Users\<u>\Prog Ă\NaVo` (mixed case), with `--silent --log <file>`. List the files, the HKCU key under the user's SID, and the links with their targets. Then write a test `navo-install.json` into the root, as IW7 does after verification. Pass: exit 0, the root keeps its file id, and Setup's log lands in `<file>`.
    - P1h: P1 with the hold on the empty root and on the created parent for the whole run. The test marker is written through the held root afterwards. Pass: exit 0 and P1's listing (R8).
  - Upgrade path:
    - P2: the apply steps from P1's root (0.0.2 with the test marker) to 0.0.3, Navo closed. Pass: exit 0; `sq.version` has 0.0.3; the marker and the root's file id are unchanged; `Update.exe` is the extracted one; the HKCU `DisplayVersion` is 0.0.3 and `InstallLocation` is unchanged; no UAC prompt; Navo does not start; no `%LocalAppData%\NavoApp` appears; the Navo log shows the obsolete and updated hooks, each under 15 s; `Update.exe`'s log lands in `<file>`. Record `packages\` before and after.
    - P2u: from a fresh P1 root, Navo's own update 0.0.2 → 0.0.3 through `NAVO_UPDATE_FEED_DIR`. Pass: no `%LocalAppData%\NavoApp`, and the marker survives. Then compare `Update.exe`'s hash and `packages\` with P2's: the wizard's steps must leave the state the library leaves (R9).
    - P3 (record only): the apply steps with Navo running, then with Navo started during the run. Record the hook calls, which processes are force-stopped, and the exit code. The wizard refuses the first case through IW9.
    - P3h: P2 with the hold on the existing root for the whole run. Pass: as P2 (R8).
    - P3d (record only): the apply steps with the 0.0.2 package over 0.0.3. Record whether apply downgrades; IW8's Newer check is the guard either way.
    - P3s (record only, R2's evidence): Setup.exe 0.0.3 `--silent --installto <root>` over a 0.0.2 root with the test marker, first unheld, then held. Record the renamed sibling, kills, hooks, `packages\`, the marker, re-created links and the exit code. W0 Q2 and Q11 expect: unheld, the marker is lost and the exit is 0; held, exit 1 with the root intact.
  - P4: install with both links, delete the desktop link, then upgrade through the apply steps, then through Navo's own update. Separately, the same for the Start menu link. Pass: a deleted link stays deleted (W0 Q6).
  - P5: uninstall through the `UninstallString`. A file inside the root is gone, a file beside it survives, and user data stays. Record that a user-made `Mine.lnk` on the Desktop pointing into the root is gone too (W0 Q6).
  - P6: a missing parent chain.
  - P7 (record only): Setup.exe on a full disk (a VHD of 50 MB); a killed Setup.exe; the apply steps on a root the user cannot write (a deny-write ACE). Record the exit codes and the files left in the root (W0 Q11: a fresh install is not rolled back). For apply, record any UAC prompt and where `packages\` goes (W0 Q8, Q13).
  - P8 (elevated, record only): Program Files, then an update. Record the UAC prompts and `packages`.
  - P9: a second admin account elevated. Record which profile gets the entries.
  - P10: uninstall with the index service installed. The hook prompt is accepted, then declined, then left open for 30 s.
- **Check:** a report with each probe's commands, listings and log excerpts. **Stop if:** P1 or P1h fails (R8); P2 or P3h fails, loses the marker or shows a UAC prompt (R8, IW8b); P2u leaves another state than P2 (R9); or P4 shows a re-created link after an update (IW11). P3, P3d, P3s and P7 are record only.

### W2a — `navo_diag::processes`

- **Read:** `navo-app/src/updater/apply.rs:184-360`, `apply_tests.rs:1-10,130-150,245-260`, `navo-diag/src/lib.rs`, `navo-diag/Cargo.toml`.
- **Change**, the same behaviour on Windows:
  - `navo-diag/src/processes/mod.rs`: `pub fn any_under(root: &Path) -> io::Result<bool>`, and `unreadable_counts` and `is_other` as `pub(crate)`, compiled on every platform. Off Windows, `any_under` returns `Err(io::ErrorKind::Unsupported)`; a scan error counts as running (IW9).
  - `processes/windows.rs`: the ToolHelp scan, moved as is. It starts with `#![allow(unsafe_code, reason = "process enumeration requires Win32 FFI")]`, as `memory/windows.rs:1-4` does; the crate keeps `#![deny(unsafe_code)]`. Every `unsafe` block keeps its `// SAFETY:` line.
  - `processes/tests.rs`: copies of `apply_tests.rs:136-149` and `249-256`.
  - Registrations: `pub mod processes;` in `lib.rs`; `Win32_System_Diagnostics_ToolHelp` and `Win32_System_RemoteDesktop` in `Cargo.toml`'s Windows `windows-sys` features.
- **Check:** also `cargo xtask check -p navo-diag` on Windows. The manual Linux CI covers the off-Windows stub when it next runs.

### W2b — The updater uses it

- **Change:** the Windows `others_running` in `apply.rs` calls `navo_diag::processes::any_under`, and the updater's own `mod processes`, `unreadable_counts` and `is_other` are deleted. The non-Windows `others_running` stub (`apply.rs:190-194`) stays as it is. `apply_tests.rs` drops their imports and tests (lines per G1); `apply_process_query_without_a_root_or_off_windows_reports_running` stays.
- **Stop if:** any other test changes.

### W3 — Crate scaffold

- **Change:**
  - Create `crates/navo-setup/Cargo.toml` with IW14's dependencies and build dependencies (path dependencies as in `navo-app/Cargo.toml`, versions as listed), `proptest = "1.12.0"` and `tempfile = "3.27.0"` as dev-dependencies, `[features] tester = []` and `[lints] workspace = true`.
  - Windows `windows-sys` features: `Win32_Foundation`, `Win32_Globalization`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_Storage_FileSystem`, `Win32_System_Com`, `Win32_System_Console`, `Win32_System_LibraryLoader`, `Win32_System_Registry`, `Win32_System_Threading`, `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`. `windows` features: `Win32_Foundation`, `Win32_System_Com`, `Win32_UI_Shell`, `Win32_UI_Shell_Common`. A later atom that needs one more feature adds that one line and names it in its report.
  - Create `src/main.rs`. With the `tester` feature, `tester::dispatch(&args)` runs first. `--version` prints `navo-setup <CARGO_PKG_VERSION>`. Anything else exits 2.
  - Create `src/tester.rs` (`#[cfg(feature = "tester")]`): `pub fn dispatch(args: &[OsString]) -> Option<ExitCode>` matches `--tester-probe <kind> …` and returns `None` otherwise. W3 leaves only the fallback arm; W7a–W7d each add one arm.
  - Add no workspace edit; the glob covers the crate.
- **Stop if:** a pinned version is missing from `Cargo.lock`.

### W4a — Path syntax

- **Read:** IW6.
- **Change:** `src/syntax.rs` and `src/syntax_tests.rs`. `Candidate(PathBuf)` and `FolderProblem`, one variant per IW6 and IW7 rule, each with `message()` and `fix()`. Tests:
  - one per rule;
  - `C:\a\..\b`, `C:\a\.\b`, `C:\a.`, `C:\a `, `C:\a:b`, `CON`, `\\?\C:\x`, `\\srv\s`;
  - a 151-unit path;
  - `C:\a%TEMP%b\Navo`, `C:\R&D\Navo`, `C:\a^b\Navo` and `C:\Hi!\Navo` refused, each message naming its character;
  - non-ASCII and spaces accepted;
  - proptest: never panics, and anything it accepts round-trips.

### W4b — Placement

- **Read:** IW7, IW7a, IW8a.
- **Change:** `src/placement.rs` and `src/placement_tests.rs`.
  - `trait NameEq { fn same(&self, a: &OsStr, b: &OsStr) -> bool }`. The probe implements it with `CompareStringOrdinal` (W7a); these tests use an ASCII fake.
  - `Chain { existing: Vec<Link { path, name: OsString, id: FileId, reparse: bool }>, missing: Vec<OsString> }`.
  - `Protected { anchor: FileId, anchor_case_sensitive: Option<bool>, tail: Vec<OsString> }`; `tail` is empty when the folder exists, and `None` means unknown, which refuses.
  - `Refused { never_inside: Vec<Protected>, never_equal: Vec<Protected>, fresh_only: Vec<Protected> }`.
  - `Facts { drive: DriveKind, subst: bool, writable: bool, free: u64 }`.
  - `check()` fails closed.
  - Tests: a reparse at each depth, an identity match through a different spelling, inside versus equal, a root that exists, remote, removable, not writable, low space, and `browse_target`.
  - Clean-profile tests, with `%LocalAppData%\Navo` and `%AppData%\Navo` absent: `…\Local\Navo`, `…\Local\NAVO\x\Navo`, `…\Roaming\Navo` and `…\Local\NavoApp` are refused; `…\Local\Programs\Navo` and `…\Local\NavoTools\Navo` are accepted.
  - A `subst` drive is refused. An anchor that is case-sensitive, or of unknown case sensitivity, refuses every candidate under it.

### W5 — Install mode and marker

- **Change:** `src/mode.rs`, `src/marker.rs`, `src/mode_tests.rs`.
  - `parse_sq_version` caps input at 64 KiB and uses `roxmltree`. It reads `package/metadata/id` and `version` (namespace-agnostic); the version goes through `navo_update::parse_version`.
  - The marker is read and written with `serde_json`, at most 4 KiB, with unknown fields refused.
  - `mode()` follows IW8, with every Ambiguous reason.
  - Tests cover each mode and reason, prerelease ordering (`0.4.0-beta.1 < 0.4.0`), and proptest (never panics on any input).
  - Downgrade guard (IW8): installed 0.0.3 with payload 0.0.2 is Newer, and so is installed `0.4.0` with payload `0.4.0-beta.1`.
  - One install (IW8): a registry key alone, or the legacy folder alone, never gives Fresh. A root with `%`, `^`, `&` or `!` gives `Ambiguous(UnsafeName)`.
  - No fuzz target, by decision: the parser is a thin layer over `roxmltree`, and the repo has no fuzz setup.

### W6 — Flow

- **Change:** `src/flow.rs` and `src/flow_tests.rs`.
  - The pages are Welcome, Licence, Folder, Shortcuts, Installing, Finish, Failed, Blocked, Same, Newer, Ambiguous, Elevated and NoPayload.
  - The effects are `CheckFolder(Candidate)`, `Install(Request)`, `RescanRunning`, `StartNavo(Root)` and `Close`.
  - Tests:
    - Next is refused without the licence ticked or with a folder problem.
    - Close and Back are refused while Installing.
    - A second `Install` is impossible until `SetupExited`.
    - Esc before the install gives only `Close`.
    - Elevated and NoPayload are terminal pages.
    - Upgrade shows the folder locked and skips Shortcuts; only Fresh has an editable folder.
    - Failed with a non-empty root has Retry off.
    - Results tagged with an older candidate are dropped.

### W7a–W7d — Windows probes

They run in order, W7a → W7b → W7c → W7d. Each later atom adds one `mod` line to `probe/mod.rs` and one arm to `tester.rs`, so none run in parallel.

- **W7a:** `src/probe/mod.rs` and `src/probe/chain.rs`.
  - Each existing ancestor is opened with `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`; `FileAttributeTagInfo` gives the reparse flag and `FileIdInfo` the identity. The handle closes after the read.
  - `refused()` builds the IW8a sets through `SHGetKnownFolderPath`, with absent folders as an anchor plus missing names. It reads each anchor's `FileCaseSensitiveInfo`; an error gives `None`.
  - `OrdinalNames` implements `NameEq` with `CompareStringOrdinal(…, TRUE)`.
  - Windows test `names_agree_with_ntfs`: for each pair, create the first name in a temp folder, then call `CreateDirectoryW` with the second. Whenever NTFS answers `ERROR_ALREADY_EXISTS`, `OrdinalNames` must say same. Pairs: `navo`/`NAVO`, `ä`/`Ä`, `ß`/`ẞ`, `ı`/`I`, `i`/`İ`, `ǅ`/`Ǆ`, `σ`/`Σ`, `ς`/`Σ`, `K`/Kelvin sign, `Å`/Angstrom sign.
  - One arm in `tester.rs`: `--tester-probe chain <path>` prints the chain and the verdict as JSON.
  - VM table T1: `C:\`; a junction in the chain; an 8.3 short name (`C:\Users\u\APPDAT~1\Local\Navo`); an OneDrive folder; a redirected Documents (set through the registry in a VM snapshot); on the clean snapshot an absent `%LocalAppData%\Navo`, `%LOCALAPPDATA%\NAVO` and `%LocalAppData%\NavoApp`. A case-sensitive anchor row (`fsutil file setCaseSensitiveInfo`) runs where `fsutil` allows it and is logged as skipped otherwise. Each gives the expected verdict.
- **W7b:** `src/probe/facts.rs`.
  - `GetDriveTypeW`; `QueryDosDeviceW` on the drive letter (`subst` when the target starts with `\??\`; an error counts as `subst`); `GetDiskFreeSpaceExW`.
  - A write probe `.navo-setup-probe-<hex>`: `CREATE_NEW`, `FILE_FLAG_DELETE_ON_CLOSE`, share none, closed at once.
  - `elevated()` through `TokenElevation`; an error means elevated.
  - One arm: `--tester-probe facts <path>`. VM table T2: a normal user, an elevated user, a read-only folder, `subst X: C:\Windows` with `X:\Navo`, and `subst Y: C:\Users\u\Prog` with `Y:\Navo` (a harmless target, still refused).
- **W7c:** `src/probe/discovery.rs`. The registry key and legacy folder per IW8 feed `mode()`; a root on a `subst` drive is not valid. One arm: `--tester-probe discover`. VM table T3 covers each IW8 outcome, set up by hand-made folders and registry keys in a snapshot.
- **W7d:** `src/probe/pin.rs` and `src/probe/pin_tests.rs` (Windows), per IW7a.
  - `Pin::fresh(&Chain)` creates and holds each missing folder and refuses a reparse tag. `Pin::existing(root, expected: FileId)` holds an existing root and refuses another identity.
  - `ancestors_unchanged(&Chain)` re-reads the existing ancestors by path. `unchanged()` re-reads the held folders through their handles. `remove_created(self)` runs IW7a's cleanup.
  - Tests in a temp folder:
    - renaming a held root or a created parent fails; a renamed existing ancestor is caught by `ancestors_unchanged`;
    - an identity mismatch refuses; a sharing violation at open refuses;
    - a second open of the held root with share read and write succeeds (the Setup.exe case);
    - `remove_created` deletes the empty created folders, deepest first, and keeps a root that holds any file, all its parents, and every folder it did not create.
  - One arm: `--tester-probe pin <path> <seconds>` holds an existing root. VM table T5: while it holds, `Rename-Item` on the root fails; after it exits, the rename succeeds and is undone (the positive control).
- Every `unsafe` block has a `// SAFETY:` line.

### W8a — Payload

- **Change:** `build.rs` (the resource, the manifest per IW13, the Setup.exe and the nupkg per IW15; build dependencies from W3), `src/payload.rs` and `src/payload_tests.rs`, per IW15a. `Payload { setup_sha256, setup_size, package_sha256, package_name, version: Version }`. Windows tests:
  - an embedded payload has `CARGO_PKG_VERSION`; a file payload has the version it was given;
  - a nupkg whose `.nuspec` has another id or version refuses (exit 9);
  - the DACL holds only the user and SYSTEM;
  - a hash mismatch refuses to run;
  - an existing work-folder name is retried with a new one;
  - a second open for write fails while the handle is held;
  - cleanup deletes `Setup.exe` and the work folder by handle, and keeps a file whose id differs;
  - cleanup keeps the logs; rotation keeps 5 and skips a reparse point.
- **Stop if:** `embed_resource` cannot take the manifest.

### W8h — DLL guard

- **Change:**
  - `build.rs` adds the IW24 link arguments. The delay-load list is the DLLs that `dumpbin /imports` shows on the W8a build, minus KnownDLLs; the worker lists them in the atom report.
  - `main.rs` makes the `SetDefaultDllDirectories` call its first line.
  - New `scripts/setup-imports.ps1`.
- **Stop if:** gpui fails at start with delay-loading. Report which DLL.

### W8u — Upgrade preparation

- **Read:** IW3, IW8b, IW15, IW15a; the `velopack` crate's `manager.rs:463-475` and `615-670`, in the cargo registry.
- **Change:** `src/upgrade.rs`, `src/upgrade_tests.rs`, and one `mod upgrade;` line in `main.rs`.
  - `writable(&Pin)`: IW8b step 2's probe in the root.
  - `stage_package(&Pin, &Payload)`: IW8b step 4. It returns the staged path.
  - `replace_update_exe(&Pin, &Payload)`: IW8b step 5 through `zip`. It returns the SHA-256 for IW4 check 2.
  - `apply_args(&Root, &Staged, log)`: IW3's exact upgrade array.
  - Leftover `.navo-partial` and `.navo-new` files are deleted by handle after a failure.
  - Windows tests in a temp root: the staged package has the payload's hash; an existing package of the same name is replaced; a nupkg with no `Squirrel.exe` entry, or two, refuses; an `Update.exe` opened without share delete makes the replace fail and keeps the old file; `apply_args` equals the library's list plus `--log`; a read-only root gives `writable == false`.
- **Stop if:** the real 0.0.3 nupkg has no `Squirrel.exe` entry, or more than one.

### W8b — Runner

- **Change:** `src/runner.rs`, `src/runner_tests.rs` and `src/host.rs`.
  - `trait Host` holds the platform steps: `elevated`, `discover`, `pin_fresh`, `pin_existing`, `ancestors_unchanged`, `writable`, `running`, `extract`, `stage_package`, `replace_update_exe`, `registry_root`, `spawn` (returns `Box<dyn Child>` with `wait_timeout` and `wait`), `verify_files`, `write_marker`, `read_marker` and `cleanup`. `host.rs` implements it with the probes, `Pin`, `payload`, `upgrade` and `navo_diag::processes`.
  - `Session` owns the `Pin`, the marker's file id and the extracted payload. The recovery errors `Unmarked(Session)`, `Remarked(Session)` and `Unlisted(Session)` carry the session, so its handles stay open while the Failed page shows.
  - `install()` runs in this order:
    1. elevation check;
    2. discovery; Same and Newer stop here, and a fresh install needs no install found (IW8);
    3. hold (IW7a): a fresh install creates and holds; an upgrade holds the existing root and checks its identity from discovery;
    4. recheck the ancestors and the refused set;
    5. upgrade: the write check and the running check (IW8b steps 2 and 3); a block writes nothing;
    6. fresh: extract Setup.exe to the work folder; upgrade: stage the package and replace `Update.exe` (W8u);
    7. recheck the ancestors; fresh: the HKCU key is still absent; upgrade: the running check again; then spawn IW3's array;
    8. wait, with progress events every second and the "still working" event at 180 s;
    9. verify IW4 checks 1–3, 5 and 6;
    10. fresh: write the marker (IW7); upgrade: check the marker, or write it for a legacy root that has none;
    11. on success, close the session. On a failed fresh install, IW7a's cleanup removes the empty created folders and names a root that is not empty. An upgrade deletes nothing but its own leftovers (IW8b).
  - `mark_again(&Session)` checks `unchanged()`, then writes the marker with `CREATE_NEW`. A file already at that path gives `Remarked` and is never overwritten.
  - Fake-host tests:
    - success, fresh and upgrade; exit ≠ 0 from Setup.exe and from apply; a child that outlives 180 s and then exits;
    - the spawned arrays equal IW3's, for both modes;
    - an ancestor changed before spawn (refused, nothing spawned) and after the exit (failed, files kept); an upgrade root whose identity differs from discovery's (refused, nothing spawned); a held folder changed after the exit (failed, files kept);
    - a running Navo at the first check (nothing written) and at the second (nothing spawned); a scan error; a missing `sq.version`; a version other than the payload's;
    - an upgrade root that is not writable (exit 3, nothing written or spawned); a failed `Update.exe` replace (exit 5, apply not spawned);
    - Same and Newer: nothing held, written or spawned (the downgrade guard);
    - a fresh `--installto` while a key or the legacy folder exists (exit 3, nothing created); a key that appears before spawn (exit 6, empty folders removed);
    - a failed Setup.exe that wrote nothing (empty folders removed, Retry on) and one that wrote a file (root kept and named, Retry off);
    - after exit 0: the marker write fails (`Unmarked`; the session stays open; `mark_again` writes it); a file already at the marker path (`Remarked`, kept); no registry entry (`Unlisted`, the text names `Update.exe uninstall`). Nothing installed is deleted in any of them, and Close only drops the session;
    - a legacy upgrade whose marker write fails (success, logged);
    - a Retry that reuses the session's empty root.

### W8s — Shortcuts

- **Change:** `src/shortcuts.rs` and `src/shortcuts_tests.rs` per IW11, using `IShellLinkW` and `IPersistFile`. Windows tests in a temp "desktop": a link into the root is deleted; a link elsewhere is kept; a link that is not a `.lnk` file is kept.

### W8c — CLI

- **Change:** `src/cli.rs`, `src/cli_tests.rs`, `src/main.rs`.
  - Parse IW17 over `OsString`. In `tester` builds, also `--payload <Setup.exe> --payload-package <nupkg> --payload-version <semver>`, all three or none (IW15).
  - `--payload-sha256` and `--package-sha256` print the two hashes.
  - Silent mode runs the runner.
  - Console attach as in `navo-app/src/console.rs`.
- **Check:** VM table T4, one command per exit code (0, 2, 3, 4, 5, 6, 7, 8, 9).

## Plan-check answers (Terra, 2026-11-09)

| # | Answer |
| --- | --- |
| 1 | IW7: Setup creates a new marked folder. Files added inside later are accepted as lost, and the Folder page and guide say so. S4 tests inside and beside the root. |
| 2, 3 | IW6, IW7, IW8a: `..` rejected; identity checks on every ancestor; reparse points refused anywhere; known folders by identity, including redirected and OneDrive; fail closed. W7a T1 and S6 cover it. |
| 4 | IW8: Ambiguous, with "Repair by uninstalling first", exit 6. W7c T3 and S14. |
| 5 | IW13: elevated or unknown means refuse. W7b T2 and S7. |
| 6 | IW24: static CRT, `DEPENDENTLOADFLAG`, delay-load, `SetDefaultDllDirectories`, the imports check, S13. |
| 7 | IW15a: DACL, `CREATE_NEW`, a held handle, hash tests, accurate wording. |
| 8 | IW4, W6, W8b: one child, waited to the end, controls off, log kept. |
| 9 | Facts section split into proven and not proven; W0 Q11–Q14; W1 P3, P4, P7, P10. |
| 10 | Facts section; W1 P1 uses a mixed-case custom root. |
| 11 | IW20 states the limits; part 2's S4a–c test accept, decline and timeout. |
| 12 | IW9: the guarantee is stated; the race is accepted and documented. |
| 13 | Staging section corrected; W2b gated on VM CI. |
| 14 | W2a after W1; W7a after W4b; owner-gate column. |
| 15 | W3 dependencies complete; part 2's W12a registers `setup.ftl` in `catalog.rs`. |
| 16 | W2a and W2b name `apply_tests.rs`; part 2 recut the UI atoms with registrations. |
| 17 | G1 re-verifies after the ix8v merge. |
| 18 | W5 names the parser, version ordering and proptest. A fuzz target is declined (reason in W5). Part 2 gives commands per VM case. |
| 19 | Part 2's W14 publishes only behind a variable switched on after the VM run and review; a rehearsal input builds without publishing. |
| 20 | Part 2's W14 adds `--locked` to every release build; S11 and S12 give exact commands, restoration, the UIA dump and screenshots in both themes. |
| 21 | Second check. IW8a: absent protected folders are kept as an anchor identity plus missing names and refused by path; `%LocalAppData%\NavoApp` is fresh-only. W4b clean-profile tests, W7a T1 rows, part 2 S6 rows. |
| 22 | IW7a: every folder of the chain is pinned without `FILE_SHARE_DELETE` from the check through cleanup; a failed open refuses. W7d builds it, T5 proves it, W1 P1h/P3h and R8 cover Setup.exe. |
| 23 | IW4 and IW7a: both modes pin; an upgrade checks the root identity from discovery. W8b tests the replacement attempt; part 2 S2b renames during an upgrade. |
| 24 | IW7: the marker is written before Setup.exe and is part of success. A write failure refuses before install; a missing marker after success gets `mark_again`, never a delete. W1 P1 checks that Setup.exe keeps it. |
| 25 | IW14: the ban is on direct dependencies and code paths; `navo-ui`'s transitive `navo-index` and gpui HTTP client are accepted. W3 checks the direct list and the whole tree for `velopack`. |
| 26 | W8b runs after W2b is merged with VM CI green. |
| 27 | W2a: `processes/` module with a `windows.rs` that allows unsafe code, an `Unsupported` error off Windows, the test lines named; W2b keeps the updater's non-Windows stub. |
| 28 | IW14 and W3 list `serde`, `serde_json`, `windows`, the build dependencies and the feature lists. |
| 29 | W3 creates `tester.rs` with `dispatch`; W7a–W7d each add one `--tester-probe` arm before their VM check. |
| 30–36 | Part 2 findings; see part 2's table. |
| 37 | IW18's stale wait removed; owner gates cleared in both atom tables. |
| 38 | Human spec aligned with IW8: Same offers Open Navo and Close, Newer offers Close only. |
| 39 | W3's check uses a positive `cargo tree` listing instead of `-i velopack`, which exits 101 when the package is absent. |
| 40 | Third check. New IW0 (threat model, orchestrator). IW7 refuses any `subst` or DOS-device drive through `QueryDosDeviceW`, fail closed; W7b T2 and part 2 S6 test a mapping below a refused folder and a harmless one. The remap-during-run test is dropped (IW0). |
| 41 | Create-then-hold race: out of scope under IW0. Kept: `CreateDirectoryW`, an open with `FILE_FLAG_OPEN_REPARSE_POINT`, refusal of a reparse tag. IW7a now holds only the root and created folders; existing ancestors are checked and rechecked before spawn and after the exit. This supersedes the ancestor part of row 22. |
| 42 | Junction set on an empty held folder: out of scope under IW0. The tag is re-read through the handles after Setup.exe exits, failing closed. |
| 43 | IW7a's hold has no `DELETE` and uses read access so the share mode is enforced; W1 P1h and P3h use those exact access, flag and share values. |
| 44 | IW7a cleanup: marker first, then folders deepest first; each closed, reopened with `DELETE`, id-checked, disposed and closed before its parent. S10 `fail-empty` now passes. |
| 45 | IW7 recovery: `Session` keeps the handles while the Failed page shows; `mark_again` uses `CREATE_NEW` and never overwrites another mark (`Remarked`); a missing registry entry (`Unlisted`, IW4 condition 6) gets an `Update.exe uninstall` instruction. W8b tests each. |
| 46 | IW8a: `CompareStringOrdinal` with ignore case; case-sensitive or unknown anchors refuse. W7a tests Unicode pairs against NTFS itself, an 8.3 alias and case; IW6 keeps trailing dots and spaces out. |
| 47–49 | Part 2 findings; see part 2's table. |
| 50 | W7a–W7d run in order, since each edits `tester.rs` and `probe/mod.rs`. W10a and W13 are recut in part 2. |
| 51 | Deletion is by handle everywhere: probe (`FILE_FLAG_DELETE_ON_CLOSE`), marker and created folders (IW7a), `Setup.exe`, work folder and old logs (IW15a). |

## W0 decisions (orchestrator, 2026-11-09)

From the [W0 note](2026-11-09-installer-w0-velopack.md). Rows 64–68 are in part 2.

| # | Answer |
| --- | --- |
| 52 | IW7: the marker is written after Setup.exe exits 0 and IW4's other checks pass, through the still-held root; the root is empty before Setup.exe (W0 Q11). The pre-write and its exit-3 path are gone; this supersedes row 24 and the marker step of row 44. Cleanup removes only empty created folders; a root with partial files is named on the Failed page, and Retry is off. IW4 check 4, IW7a cleanup, `remove_created(self)`, W7d, W8b order and tests follow. |
| 53 | New IW8b: upgrades run `<root>\Update.exe apply --norestart` (R2 fired, W0 Q2). It names the write check without UAC, the running check before writing and before apply, the hooks (obsolete and updated, 15 s) and the force-stop under the root, guarded by IW9. Setup.exe runs only for fresh installs into an empty root. New atom W8u; W8b waits for it. |
| 54 | The new `Update.exe`: the wizard repeats the library's step (`manager.rs:463-475`), extracting the nupkg's `Squirrel.exe` entry over `<root>\Update.exe` through a `.navo-new` file and `MoveFileExW`. The nupkg is embedded beside Setup.exe (IW15), its `.nuspec` checked; `zip` 9.0.3 joins IW14. W1 P2u compares the result with Navo's own update (R9). |
| 55 | IW8: the Newer check is the only downgrade guard, stated. W5 tests the ordering, W8b that nothing runs, S8 that no Velopack process starts; W1 P3d records what apply does. |
| 56 | IW8: while any install is found (the key or the legacy folder), no fresh install into another folder; `--installto` elsewhere exits 3. W5, W8b and part 2's S3b test it. |
| 57 | IW6 refuses `%`, `^`, `&` and `!` anywhere (W0 Q7). Discovered roots get the same check and become `Ambiguous(UnsafeName)`, whose text never suggests uninstalling. W4a and W5 tests; part 2's S6 and S14 rows. |
| 58 | IW20: uninstall deletes every `.lnk` that points under the root, user-made ones included (W0 Q6). User-guide text in part 2's W17; W1 P5 records it. No code. |
| 59 | IW3: `--log` goes to both Velopack calls, as a separate `-velopack.log` beside the wizard's log; IW15a rotates both. W0 Q13's redirect of `packages\` to `%LocalAppData%\NavoApp` for an unwritable root is recorded; IW8b's write check refuses that case. |
| 60 | R8 covers the fresh Setup.exe with the hold on an empty root (W1 P1h) and `Update.exe apply` with the root held (W1 P3h). |
| 61 | W1 rewritten: P1/P1h fresh; P2, P2u, P3, P3h and P3d on the apply path; P3s records Setup.exe over the root for R2; P7 records an unwritable root under apply. |
| 62 | IW11 confirmed from W0 Q5 and Q6: exact `.lnk` paths; apply never re-creates a link; an upgrade skips the Shortcuts page. IW12 confirmed (W0 Q3): the Start Navo box stays. |
| 63 | IW9's scan runs twice for an upgrade, before anything is written and just before apply. A fresh install rechecks just before spawn that no HKCU key appeared. |

## W1 decisions (orchestrator, 2026-11-09)

W1 run 2 passed every stop rule (report: `testing/installer-w1-report.md`). Its new findings:

- **Free space (IW7):** the larger of 4 × payload and Velopack's own requirement; see IW7. W4b owns it.
- **Velopack's fallback folder (IW8):** `%LocalAppData%\NavoApp` is also where Velopack puts `packages\` when it cannot write the root, and uninstall never removes it. Discovery counts it as an install only when it is valid (`current\sq.version` and `Update.exe` present, as IW8 already defines). An invalid leftover is ignored for the "one install" rule and never touched. W7c adds T3 rows: a fallback folder holding only `packages\`, and one holding a partial `current\`.
- **Navo started during an upgrade (IW8b, IW9):** a Navo started between step 4 and apply finds the staged nupkg and runs its own apply (`navo-app/src/updater/apply.rs`, `at_start`). The wizard then verifies per IW4. If every check passes, it is a success, and the Finish page adds "Navo finished the update itself." Otherwise, the Failed page reads "Navo was started during Setup. Close Navo and run Setup again." and Retry is offered once no Navo runs. W8b tests both paths with the fake launcher.
- **User guide (W17):** interactive uninstall shows an "Uninstall Complete" box and removes the folder only after OK; parent folders Setup created stay after uninstall; uninstall also deletes the user's own shortcuts into the folder (IW20).

## Owner answers (2026-11-09)

- **P1 (IW18):** per-user only. Yes. Unblocks W4b and W7b; the gates are cleared in both atom tables.
- **P2 (IW19):** licence MIT OR Apache-2.0, the Rust convention. Keep "I accept". Unblocks part 2's W10b, W12b, W13a and W13b.
- **P3 (IW8):** Setup never moves an install. Unblocks W5, W6, W7c and part 2's W11b.

## Done when (part 1)

- Every atom's check passes. `--silent` installs, upgrades and refuses per T1–T5 in the VM, from a `tester` build with `--payload` and `--payload-version`.
- `setup-imports.ps1` passes. W2b is merged with VM CI green.
- No release workflow change.

## Log

- 2026-11-09 — Revised after the second Terra xhigh plan check (architect Turgon). All 21 findings checked against main `049089f3` and ix8v `caef17a7`, all accepted: rows 21–39 here and 30–36 in part 2. New: IW7a (chain pin, both modes), W7d, the marker before Setup.exe, the direct-dependency ban, the payload version. W8b now waits for W2b with VM CI green. Still drafting; the next step is a third plan check, then W0.
- 2026-11-09 — Revised after the third Terra xhigh plan check (architect Turgon). The orchestrator added IW0; findings 41 and 42 are out of scope under it, the rest fixed (rows 40–51 here, 47–49 in part 2). IW7a is simpler: it holds the root and created folders and rechecks the ancestors. W7a–W7d now run in order. Still drafting.
- 2026-11-09 — Revised from W0 (architect Turgon), rows 52–63 here and 64–68 in part 2, applying the orchestrator's eight decisions. The marker is written after verification; upgrades go through `Update.exe apply` (new IW8b and atom W8u, the nupkg embedded); IW6 refuses `%`, `^`, `&` and `!`; one install at a time; W1 probes both Velopack paths. Still drafting.
- 2026-11-09 — W1 run 1 (VM): P1, P1h, P2 and P3h PASS (the hold works on both paths; R8 cleared for them). P2u stopped on R9: the library deletes older nupkgs after a download, the wizard did not. Orchestrator: IW8b step 4 now does the same cleanup by handle; R9 restated with the compared state; Velopack's extras listed in step 8. W1 reruns P2u and the probes not yet run.
- 2026-11-09 — W1 run 2 (VM): every probe ran; no stop rule fired. P1, P1h, P2, P2u, P3h, P4, P5, P6 PASS; P3, P3d, P3s, P7, P8, P9, P10 recorded. The orchestrator added the W1 decisions (free space, the fallback folder, Navo started mid-upgrade, user guide). W1 done; part 1 may start (W2a, W3).
