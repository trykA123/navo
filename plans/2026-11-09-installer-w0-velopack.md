# W0 — Velopack's Setup.exe and Update.exe, read from source

Research note for atom W0 of `plans/2026-11-09-installer-wizard.md`. It answers Q1–Q14 from the
Velopack sources only; nothing was built or run. W1 checks the answers in the VM.

- **Repo:** `velopack/velopack`, tag `1.2.161` (lightweight tag), commit
  `92d6a1c91716729d449034df5c50307dcce39493`.
- **Version match:** Navo pins vpk 1.2.161 (`.github/workflows/release.yml:28`, `VPK_VERSION: 1.2.161`)
  and the `velopack` crate 1.2.161 (`Cargo.lock`). The upstream tag has the same name. The binaries'
  version is stamped at build time (`NGBV_VERSION`), so the tag is the only link; it matches.
- **Scope:** `src/bins/src/`, plus the lib code an answer depends on (`src/lib-rust/src/`,
  `src/l18n/src/dialogs.rs`, and vpk's C# option defaults for Q5).
- **Cites** are `velopack/<path>:<line>` at that commit. "Proven by code" means the cited lines show
  it. "Inferred" means it rests on a third-party crate (`remove_dir_all` 1.0.0, `xdialog` 3.1.9) or on
  Windows behaviour, and W1 should confirm it.
- **Pack inputs Navo uses** (`release.yml:154`): `--packId NavoApp --packTitle Navo --mainExe navo.exe`,
  no `--shortcuts`, no `--packAuthors`.

## Q1 — Setup.exe's arguments, and whether parent folders are created

Arguments (clap, `ignore_errors(true)`, so unknown arguments are ignored, not refused):

| Flag | Meaning |
| --- | --- |
| `-s`, `--silent` | No dialogs; every prompt answers "yes". |
| `-v`, `--verbose` | Debug lines on the console. |
| `-l`, `--log <FILE>` | Log file location. |
| `-t`, `--installto <DIR>` | Install root. Default `%LocalAppData%\<packId>`. |
| `-- <EXE_ARGS>` | Arguments for the started app. |
| `-d`, `--debug <FILE>` | Debug builds only. |

Cites: `velopack/src/bins/src/setup.rs:97-112`, default root `velopack/src/bins/src/commands/install.rs:38-43`.

Parents: yes. A missing root is created with `fs::create_dir_all`, so every missing parent is created
too (`velopack/src/bins/src/commands/install.rs:45-48`). Then `remove_dir_all::ensure_empty_dir(root)`
runs (`install.rs:122`). **Proven by code.**

Setup also sets its working directory to its own folder (`setup.rs:143-146`) and limits DLL search to
System32 before `main` (`velopack/src/bins/src/windows/mitigate.rs:32-39`).

## Q2 — Setup.exe over an existing NavoApp root (older, same, newer)

One path for all three versions. A root that is not empty (and has no `.dead` file) counts as
"already installed" (`velopack/src/bins/src/shared/util_common.rs:65-73`, `install.rs:94`):

1. It reads the installed version only for the dialog text (`install.rs:96-98`). With `--silent` the
   overwrite dialog returns `true` at once, so older, same and **newer (downgrade)** all proceed
   (`velopack/src/l18n/src/dialogs.rs:188-196`).
2. **Kills** every process whose image path is under the root that it can open with
   `PROCESS_TERMINATE`, with no prompt (`install.rs:106-109`,
   `velopack/src/bins/src/shared/util_windows.rs:81-102`).
3. **Renames the whole root** to a sibling `<root>.<16 random alphanumerics>` (`with_extension`, so an
   existing extension is replaced), retrying 5 times over about 4 s. Failure gives
   `RemoveExistingDirFailed` (`install.rs:111-118`, `util_common.rs:28-52`).
4. Installs fresh into the now-missing root: `ensure_empty_dir`, `Update.exe`, stubs, a new
   `packages\` holding only the new nupkg, `current\` (`install.rs:121-127`, `install.rs:199-222`).
5. On success it **deletes the renamed old root recursively**: old `packages\`, the marker, and any
   other file in it (`install.rs:150-155`). On failure it deletes the new root and renames the old one
   back (`install.rs:156-163`).

Hooks: the old version gets **no** `--veloapp-obsolete` or `--veloapp-uninstall` hook. The new version
gets `--veloapp-install <ver>` (30 s), not `--veloapp-updated` (`install.rs:244-247`). After any hook,
Setup force-stops everything under the root again (`velopack/src/bins/src/windows/util.rs:59-60`).

Shortcuts: rebuilt with no previous manifest, so deleted ones come back (see Q6). The HKCU key is
rewritten (`install.rs:250`). **Proven by code.**

## Q3 — Does Setup.exe start the app after `--silent`?

No. It starts `current\navo.exe` (with `VELOPACK_FIRSTRUN=true`) only when not silent
(`velopack/src/bins/src/commands/install.rs:252-255`). No other switch exists: in interactive mode it
always starts the app; with `--silent` it never does. **Proven by code.**

The install hook (`navo.exe --veloapp-install`) still runs in silent mode (`install.rs:244-247`). It
is not "starting Navo", but it is a Navo process launched from the root.

## Q4 — The HKCU key and its values

Key: `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<packId>`, so `...\Uninstall\NavoApp`
(`velopack/src/bins/src/windows/registry.rs:7`, `registry.rs:38-40`). Values (`registry.rs:45-56`):

| Value | Type | Content |
| --- | --- | --- |
| `DisplayIcon` | SZ | `<root>\current\navo.exe` |
| `DisplayName` | SZ | pack title (`Navo`) |
| `DisplayVersion` | SZ | short version |
| `InstallDate` | SZ | `YYYYMMDD` |
| `InstallLocation` | SZ | the root, as passed (`--installto` text) |
| `Publisher` | SZ | pack authors |
| `QuietUninstallString` | SZ | `"<root>\Update.exe" --uninstall --silent` |
| `UninstallString` | SZ | `"<root>\Update.exe" --uninstall` |
| `EstimatedSize` | DWORD | KB of `current\` |
| `NoModify`, `NoRepair` | DWORD | 1 |
| `Language` | DWORD | 0x0409 |

Setup writes it last, after the install hook (`install.rs:249-250`). A write error fails the install.

`Update.exe uninstall` (subcommand) and `Update.exe --uninstall` (long-flag alias) are the same
command (`velopack/src/bins/src/update.rs:57-61`). The plan's `"<path>\Update.exe" uninstall` form is
valid. Without `--silent` it shows a progress dialog and an "uninstall complete" dialog
(`velopack/src/bins/src/commands/uninstall.rs:18`, `uninstall.rs:44`). Uninstall finds the root as the
folder that holds the running `Update.exe` (`velopack/src/lib-rust/src/locator.rs:450-456`), and needs
`current\sq.version` there (`locator.rs:112-127`). **Proven by code.**

The key name is the pack id only. An install into a custom root overwrites the key of a legacy
`%LocalAppData%\NavoApp` install, and the reverse. **Proven by code** (`registry.rs:40`).

## Q5 — Shortcut names, folders and targets, and vpk's default `--shortcuts`

- **Default:** `Desktop,StartMenuRoot`
  (`velopack/src/vpk/Velopack.Vpk/Commands/Packaging/WindowsPackCommand.cs:87-90`,
  `velopack/src/vpk/Velopack.Packaging.Windows/Commands/WindowsPackOptions.cs:27`). Navo passes no
  `--shortcuts`, so it gets both.
- **Name:** `<packTitle>.lnk`, or `<packId>.lnk` if the title is empty
  (`velopack/src/bins/src/windows/shortcuts.rs:54-61`). For Navo: `Navo.lnk`.
- **Folders** (`shortcuts.rs:63-79`, `velopack/src/bins/src/windows/known_path.rs:26-49`):
  - Desktop: `FOLDERID_Desktop\Navo.lnk`.
  - StartMenuRoot: `FOLDERID_StartMenu\Programs\Navo.lnk` (`known_path.rs:42-46`).
  - StartMenu (not used): `...\Programs\<authors>\Navo.lnk`. Startup (not used): `FOLDERID_Startup`.
- **Target:** `<root>\current\navo.exe`. Working folder `<root>\current`. Icon `navo.exe,0`. AUMID only
  if `--aumid` was given (`shortcuts.rs:107-113`, `shortcuts.rs:210-235`).

**Proven by code.**

## Q6 — Do updates re-create a deleted shortcut? How does uninstall find shortcuts?

**Update (`Update.exe apply`): no.** Apply calls `create_or_update_manifest_lnks(new, Some(old))`
(`velopack/src/bins/src/commands/apply_windows_impl.rs:231-233`). Only locations in the new manifest
but not the old one are created; a location in both is never re-created
(`shortcuts.rs:98-101`). Existing shortcuts that point into the root are updated
(target, icon, AUMID) (`shortcuts.rs:116-161`). **Proven by code.**

**Setup.exe (fresh or over an existing root): yes.** It passes `None` as the previous app
(`install.rs:239-242`). Every manifest location without an existing shortcut into the root is
created. A Desktop shortcut the user (or the wizard) deleted therefore comes back on every Setup.exe
run. One rule limits this: any Start Menu shortcut into the root stops a new Start Menu one
(`shortcuts.rs:154-160`). **Proven by code.**

**Uninstall** (`uninstall.rs:27`, `shortcuts.rs:297-366`) scans:
- `Desktop\*.lnk`, `Startup\*.lnk`;
- `Start Menu\Programs\**\*.lnk` (recursive);
- `User Pinned\**\*.lnk` (recursive).

It deletes every `.lnk` whose target is under the root, or, when the target cannot be read, whose
working folder is. That includes shortcuts the user made. It unpins each one from Start first, and
deletes an emptied Start Menu author folder (`shortcuts.rs:368-392`). The "under the root" test
lowercases both paths, compares prefixes, and on a miss expands `%VAR%` in both before comparing again
(`velopack/src/bins/src/windows/util.rs:131-160`). **Proven by code.**

## Q7 — Does uninstall delete the root recursively, and does it follow reparse points?

Yes, recursively, in two steps (`velopack/src/bins/src/commands/uninstall.rs:20-50`):

1. Kill processes under the root, run the uninstall hook, remove shortcuts (`uninstall.rs:21-27`).
2. `remove_dir_all::remove_dir_contents(root)`: deletes everything inside, keeps the root. Errors
   are ignored (`uninstall.rs:30`).
3. Deletes `%TEMP%\velopack_NavoApp` and the HKCU key (`uninstall.rs:32-40`).
4. Spawns `cmd.exe /C choice /C Y /N /D Y /T 3 & rmdir /s /q "<root>"` with the root's parent as the
   working folder (`velopack/src/bins/src/windows/self_delete.rs:4-19`, `uninstall.rs:46-50`). This
   removes the root and whatever could not be deleted while `Update.exe` was running.

Reparse points:
- Step 2: `remove_dir_all` 1.0.0 removes a symlink or junction as a link and does not descend into it.
  **Inferred** (crate behaviour, not in this repo).
- Step 4: `rmdir /s` removes a junction or directory symlink inside the tree without following it.
  **Inferred** (Windows behaviour).
- If the root itself is a junction, step 2 empties the target. **Inferred.** IW7 refuses reparse points
  anywhere in the chain, so this cannot happen for a Setup-made root.

**New hazard, proven by code:** step 4 passes the root to `cmd.exe /C` as raw text
(`self_delete.rs:7`, `raw_arg` at `self_delete.rs:15`). `cmd` expands `%NAME%` even inside quotes
(**inferred**, documented cmd behaviour). A root such as `C:\%HOMEPATH%` would make the delayed
`rmdir /s /q` run on another folder, here `C:\Users\<u>`. `is_sub_path` also expands `%VAR%`
(`util.rs:149-150`), so the process kill and shortcut removal can widen too. IW6 does not refuse `%`
today.

## Q8 — Does `Update.exe apply` elevate itself for a root it cannot write?

Yes. If `<root>\.velopack_dir_test` cannot be created (`velopack/src/bins/src/windows/util.rs:112-129`)
and the process is not elevated, it relaunches itself with `ShellExecuteExW` verb `runas` and waits up
to 10 minutes. The relaunch passes `apply --norestart --package <p> --rootDir <root> --packageDir <dir>`
(`velopack/src/bins/src/commands/apply_windows_impl.rs:62-107`,
`velopack/src/lib-rust/src/process_win.rs:261-296`). This path ignores `--silent`: no check guards
it, so a UAC prompt can appear in silent mode. If the process is already elevated, apply fails.
**Proven by code.**

Related: when the root is not writable, the locator moves `packages\` and `Update.exe` to
`%LocalAppData%\<packId>\` (`velopack/src/lib-rust/src/locator.rs:153-176`). See Q13.

## Q9 — Setup.exe's exit codes

Only `0` and `1`: `std::process::exit(if result.is_ok() { 0 } else { 1 })`
(`velopack/src/bins/src/setup.rs:56-61`). **Proven by code** for the return path.
`xdialog`'s `run_result` passing the result through unchanged is **inferred**.

- **0:** success. Also 0 for: `--help`/`--version` (`setup.rs:69-77`); prerequisites declined
  (`install.rs:32-35`); the overwrite dialog cancelled in interactive mode (`install.rs:99-103`).
  In silent mode the overwrite dialog is never cancelled.
- **1:** any error: Windows too old, no embedded bundle, too little disk space, OS or CPU unsupported,
  kill failure, rename failure, lock taken, extract failure, `navo.exe` missing, registry write failure
  (`setup.rs:153-155`, `setup.rs:192`, `install.rs:59-127`, `install.rs:205-250`).
- A failed install hook is **not** an error: a warning dialog only, hidden in silent mode
  (`install.rs:245-247`).
- A panic gives Rust's default 101. **Inferred.**

So a 0 does not prove installed files. IW4's file and registry checks are needed.

## Q10 — Where the logs go

- **Setup.exe:** `--log <FILE>` if given (`setup.rs:101`, `setup.rs:123-127`). Otherwise
  `%LocalAppData%\velopack\velopack.log`, because Setup passes no locator
  (`velopack/src/lib-rust/src/logging.rs:10`, `logging.rs:14-21`, `logging.rs:57-72`). If LocalAppData
  is unavailable, `%TEMP%\velopack.log`.
- **Update.exe:** `--log <PATH>` if given (`update.rs:47`, `update.rs:182-184`). Otherwise
  `%LocalAppData%\velopack\velopack_NavoApp.log`, or `velopack.log` if no locator is found.
- **File handling:** the file is opened for append and its parent folders are created. Above 1 MB it
  is renamed to `<file>.old` (`logging.rs:110-120`, `velopack/src/lib-rust/src/file_rotate.rs:43`,
  `file_rotate.rs:56-57`, `file_rotate.rs:65-72`). File level is Info, or Trace with `--verbose`.

**Proven by code.** IW3's `--log <log file>` is confirmed. The wizard should expect a possible
`<log>.old` beside it.

## Q11 — Does Setup.exe accept a pre-created empty root?

**An empty root: yes.** `create_dir_all` is skipped (`install.rs:46-48`). `is_dir_empty` is true
(`util_common.rs:65-73`), so there is no rename. `ensure_empty_dir` keeps the existing empty folder
(`install.rs:122`; keeping rather than recreating is **inferred** from the crate). **Proven by code**
for the branch.

**A root holding the marker: no.** A single file makes the root "not empty". Setup then takes Q2's
existing-install path (`install.rs:94-119`):
- **Without a hold:** the root is renamed away and the new root is created fresh. On success the
  renamed folder, **with the marker**, is deleted (`install.rs:150-155`). The marker is lost and exit
  is 0.
- **With IW7a's hold:** the root is held without `FILE_SHARE_DELETE`. The rename fails after 5 tries,
  giving `RemoveExistingDirFailed` and exit 1 (`install.rs:114-116`). The failure from the sharing
  conflict is **inferred**; W1 P1h proves it.

Also: an empty root with a `.dead` file counts as empty. A failed fresh install into an empty root is
not rolled back, because `root_path_renamed` is `None`. Partial files then stay in the root
(`install.rs:156-165`).

## Q12 — Does an update or apply keep unknown files in the root (the marker)?

**`Update.exe apply`: yes.** It renames only `current\` to a temp folder and the new tree into
`current\` (`apply_windows_impl.rs:129-131`, `apply_windows_impl.rs:162`,
`apply_windows_impl.rs:175`). In the root it writes only:
- `Update.exe`, copied when apply runs from elsewhere (`apply_windows_impl.rs:238-267`);
- the stub exes (`apply_windows_impl.rs:269-270`);
- `.velopack_dir_test`, removed on close (`util.rs:112-129`).

Temp folders are under `packages\VelopackTemp` (`locator.rs:206-213`). `<root>\navo-install.json`
survives. **Proven by code.**

Apply does not install the new `Update.exe` from the package. The library does that at download time
(`velopack/src/lib-rust/src/manager.rs:463-475`). The `Update.exe apply` fallback must therefore
replace `Update.exe` itself, or keep the old one. **Proven by code** (no `Squirrel.exe` extraction in
`apply_windows_impl.rs`).

**Setup.exe over the root: no.** See Q2 and Q11: the old root is deleted, marker and all.

## Q13 — Does `Update.exe` in a custom root assume `%LocalAppData%\NavoApp` in the registry?

**Registry: no.**
- Setup, apply and uninstall write or delete only `HKCU\...\Uninstall\NavoApp`.
- The values come from the locator's root, which is the folder of the running `Update.exe`
  (`registry.rs:9-85`, `apply_windows_impl.rs:209-215`, `uninstall.rs:38-40`).
- No bins or lib code reads a registry value to find the root. The only other registry reads are HKLM
  runtime checks (`velopack/src/bins/src/windows/runtimes.rs:149`, `runtimes.rs:230`).

**Proven by code** (grep of `RegKey`/`HKEY_` in `src/bins/src` and `src/lib-rust/src`).

**File system: two `%LocalAppData%` assumptions.**
- Logs go to `%LocalAppData%\velopack\` (Q10).
- If the root is not writable, `packages\` and `Update.exe` fall back to `%LocalAppData%\NavoApp\`
  (`locator.rs:153-176`). That fallback would create or reuse the legacy folder's `packages\` and
  `Update.exe`. A per-user root made by the wizard is writable, so this applies only if permissions
  change later. **Proven by code.**

## Q14 — Do the uninstall callback's exit code or a timeout change what uninstall does?

No. `run_hook(locator, "--veloapp-uninstall", 60)` returns a bool that uninstall ignores
(`uninstall.rs:24`). `run_hook` (`velopack/src/bins/src/windows/util.rs:19-61`):
- **Non-zero exit:** logs a warning.
- **Timeout after 60 s:** kills the hook process and logs an error.
- **Either way:** force-stops every process under the root afterwards.

Uninstall then removes shortcuts, files and the key regardless (`uninstall.rs:27-40`). Navo's hook
budget of 25 s fits inside the 60 s. A UAC prompt left open by the hook does not survive past 60 s.
The hook process is killed, but an elevated child it started cannot be opened for terminate and lives
on (**inferred**). **Proven by code** for the bool being ignored and for the timeout kill.

Other hook timeouts:
- install: 30 s, failure gives a warning only (`install.rs:245`);
- apply obsolete: 15 s (`apply_windows_impl.rs:153`);
- apply updated: 15 s (`apply_windows_impl.rs:219`).

None of them changes the outcome.

## Impact on the plan

| Decision | Verdict | Why |
| --- | --- | --- |
| **IW3** | Confirmed | `--installto`, `--silent` and `--log` exist with these meanings (Q1, Q10). Unknown flags are ignored silently, so a typo is not caught. |
| **IW4** | Confirmed, one addition | Exit codes are only 0/1, and 0 can mean nothing was installed (Q9), so the file and registry checks are needed. Check 6: `InstallLocation` is the root text exactly as passed to `--installto` (Q4). Check 4 fails for every Setup.exe upgrade (see IW8). |
| **IW7** | **Contradicted** | The marker written before Setup.exe makes the root "not empty". Without a hold, Setup renames the root and deletes the marker on success. With the IW7a hold, Setup fails with exit 1 (Q11). The plan's own fallback applies: write the marker after Setup.exe exits and verification passes, into the still-held root. Separately, a failed fresh install leaves partial files (Q11), so "a Setup.exe that wrote nothing leaves nothing" holds only when it truly wrote nothing. Uninstall's recursive delete is confirmed (Q7). |
| **IW6** | **Gap** (not on the list, but load-bearing) | Uninstall runs `rmdir /s /q "<root>"` through `cmd.exe /C`, and `is_sub_path` expands `%VAR%` (Q7). A root containing `%` can make uninstall delete another folder. IW6 should refuse `%` in any component. Refusing `^`, `&` and `!` too costs nothing. |
| **IW7a** | **Contradicted for upgrades; confirmed for fresh** | Fresh with an empty held root: Setup only creates inside it, so the hold is compatible (W1 P1h still proves it). Upgrade through Setup.exe renames the held root, which must fail (Q2, Q11). |
| **IW8** | **Contradicted** | "Upgrade in place" with Setup.exe kills Navo with no prompt, wipes `packages\`, deletes the marker and any user files, and re-creates deleted shortcuts. It fails outright with the root held (Q2). Also, in silent mode Setup.exe downgrades a newer install without asking, so the wizard's Newer refusal is the only guard. Setup.exe into a custom root overwrites the legacy `%LocalAppData%\NavoApp` registry key (Q4). |
| **R2** | **Fires** | The R2 fallback, `Update.exe apply`, keeps the root, the marker and deleted shortcuts (Q6, Q12). It runs the obsolete (15 s) and updated (15 s) hooks and force-stops processes under the root. It does not replace `Update.exe` (Q12). It may raise UAC even with `--silent` if the root is not writable (Q8). Pass `--norestart`, or it starts Navo. Navo 0.4.x's own hooks must be checked against these calls (W1 P3). |
| **R8** | **Fires for upgrades only** | Q2 and Q11 show Setup.exe renames a non-empty root. With the `apply` fallback the root is never renamed: apply renames only `current\`, which the wizard does not hold (inferred; W1 P3h proves it). |
| **IW11** | Confirmed for updates, **contradicted for Setup.exe runs** | Updates never re-create a deleted shortcut (Q6). Every Setup.exe run re-creates missing ones, so with the apply fallback the wizard's delete-after-success stays valid. The `.lnk` paths are `FOLDERID_Desktop\Navo.lnk` and `FOLDERID_StartMenu\Programs\Navo.lnk` (Q5). Uninstall deletes every `.lnk` targeting the root, user-made ones included. |
| **IW12** | Confirmed | `--silent` never starts Navo, and it cannot be told to (Q3). Keep the Start Navo box. The install hook still runs `navo.exe --veloapp-install` during Setup. |
| **IW20** | Confirmed | The uninstall hook gets 60 s, and its result never stops the uninstall (Q14). |
