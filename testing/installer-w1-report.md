<!-- W1 of the installer wizard plan: VM probe of Velopack's fresh and upgrade paths, with evidence and plan impact. -->
# Installer W1: VM probe of Velopack's two paths

Date: 2026-11-09 (Europe/Bucharest). Plan: `plans/2026-11-09-installer-wizard.md`, atom W1. Checkout: main `33203a83`.

**Result: the R9 stop rule fired at P2u.** P1, P1h, P2 and P3h pass. P2u leaves a different `packages\` than P2. As W1's Check line requires, the run stopped there: evidence was collected, the VM was cleaned, and P3, P3d, P3s and P4–P10 were not run.

| Probe | Verdict |
| --- | --- |
| P1 | PASS |
| P1h | PASS |
| P2 | PASS |
| P3h | PASS (run before P2u, on P1h's root) |
| P2u | **FAIL: R9 stop rule** (`packages\` differs from P2) |
| P3, P3d, P3s, P4, P5, P6, P7, P8, P9, P10 | SKIPPED: not run, because the R9 stop rule fired at P2u |

## Setup

**VM.** Windows 11 Pro (10.0.26300). The VM has one enabled account, `WINVM\admin`, a member of Administrators. UAC is on (`EnableLUA=1`, `ConsentPromptBehaviorAdmin=5`, secure desktop). "The standard user" is admin's limited token (Medium integrity, checked with `whoami /groups`). It runs through an interactive scheduled task (`-LogonType Interactive -RunLevel Limited`) in session 1, as in the IX8r and IX8v runs. Every VM step held the shared VM lock.

**Driver.** Each probe is a PowerShell script in `C:\winvm\installer-w1\probes\`. It dot-sources `lib.ps1` and runs as the limited user through `run.ps1`. `run.ps1` runs elevated over SSH. It registers the task, waits for the probe to finish, and polls `consent.exe` every 200 ms, so it counts UAC prompts. Logs are in `C:\winvm\installer-w1\logs\`.

- **The hold** is IW7a's exact handle, opened through `Add-Type`. It uses `CreateFileW(path, FILE_LIST_DIRECTORY|FILE_READ_ATTRIBUTES|SYNCHRONIZE, FILE_SHARE_READ|FILE_SHARE_WRITE, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT)`.
- **File ids** come from `GetFileInformationByHandleEx(FileIdInfo)`: the volume serial and the 128-bit id. Reparse tags come from `FileAttributeTagInfo`.
- **The marker through the held root** is written with `NtCreateFile`. `RootDirectory` is the held handle, the name is `navo-install.json`, and the call uses `FILE_CREATE` and write-through. The data is then flushed.
- **The apply steps** follow IW8b steps 4–6:
  1. Copy the full nupkg to `packages\<name>.navo-partial` with `CreateNew`, and check its SHA-256 through that handle.
  2. Rename it to `<name>` with `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)`.
  3. Extract the single nupkg entry that ends in `Squirrel.exe` (`lib/app/Squirrel.exe`) to `Update.exe.navo-new` with `CreateNew`.
  4. Rename it over `Update.exe` with the same flags.
  5. Run IW3's array.

**Packages.** No cargo build was needed. The packages came from the existing v0.4.0 tester build in the VM (`C:\winvm\ix8v-91e7a4e7\dist`, commit `91e7a4e7`). Between `91e7a4e7` and `33203a83`, `crates/` and `.github/` are unchanged; the only change is one line in `scripts/index-service-e2e.ps1`. vpk 1.2.161 packed three versions with Navo's release arguments (`--packId NavoApp --packTitle Navo --mainExe navo.exe --channel win --runtime win-x64`, no `--shortcuts`) into `C:\winvm\installer-w1\packages\<ver>\`.

| File | SHA-256 |
| --- | --- |
| dist `navo.exe` (tester) | `106A10DCBFC6D92A153E6CD5AAEB8FF973BA6D905426121031C125E655B75372` |
| 0.0.2 `NavoApp-win-Setup.exe` | `92E28AA8A7EA66B4E336EF9043DDDC1381714E8B0D8B4055E7682E884369EC50` |
| 0.0.2 `NavoApp-0.0.2-full.nupkg` | `DEFCF5C9E6DBC6A0BAD6C4974CD1B920DCCCB4C4AD28088C16183D3990D39912` |
| 0.0.3 `NavoApp-win-Setup.exe` | `E67B6FDA8D981B922AB6BFBFE50B8839F35045F5D5A643614ED3689CC875EFCE` |
| 0.0.3 `NavoApp-0.0.3-full.nupkg` | `FA5652381D4D81210C176D2C14246A1553C04B19CAB645BF1A747D8FDD042C19` |
| 0.0.4 `NavoApp-0.0.4-full.nupkg` | `85150583A7F9E6F01901DB487A3993AF1C08C4322833D98C71E5058C4B41DB14` |
| `Update.exe` from every pack (`lib/app/Squirrel.exe`) | `B06A2474766B71CF177283783DE78D690B674B1A088394DC44DE345943F340C0` |

All packs come from the same vpk, so their `Update.exe` is byte-identical. A hash comparison therefore cannot tell "replaced" from "kept". The file id and modification time show which one happened (P2, P2u).

**State before (P0).** There was no `Uninstall\NavoApp` key under `HKU\S-1-5-21-…-1000`, no `Navo.lnk` and no `%LocalAppData%\NavoApp`. There was no Navo folder under Program Files and no `Navo Index` service. The older data folder `C:\Users\admin\AppData\Local\Navo` was listed (18 files; `logs\navo.log` was 21178 bytes) and left alone.

The test root in every probe is `C:\Users\admin\Prog Ă\NaVo`, with `Ă` = U+0102. The SSH console shows it as `Prog A`. `Show-Exact` prints it as `Prog U+0102`, which confirms the real name.

## P1 — Setup.exe into a pre-created empty root: PASS

Command (limited user):

```
New-Chain → CreateDirectoryW "C:\Users\admin\Prog Ă", then "…\Prog Ă\NaVo"   (empty)
packages\0.0.2\NavoApp-win-Setup.exe --installto "C:\Users\admin\Prog Ă\NaVo" --silent --log C:\winvm\installer-w1\logs\P1-velopack.log
then navo-install.json written by path with CreateNew (test marker)
```

Evidence:

```
root id before=7436BA7636BA3946:26781D0000002F000000000000000000
EXIT 0 after 1.69s
root id after =7436BA7636BA3946:26781D0000002F000000000000000000 same=True
  dir  current  19 files
  file Navo.exe  480256                (Velopack's execution stub)
  file navo-install.json  101
  dir  packages  2 files               (.velopack_lock 0 B, NavoApp-0.0.2-full.nupkg)
  file Update.exe  5219840  sha256=B06A2474…F340C0
  sq.version id=NavoApp version=0.0.2
HKCU (HKU\S-1-5-21-…-1000\…\Uninstall\NavoApp):
  DisplayVersion = 0.0.2
  InstallLocation = C:\Users\admin\Prog U+0102\NaVo   (-ceq the --installto text: True)
  UninstallString = "C:\Users\admin\Prog Ă\NaVo\Update.exe" --uninstall
  QuietUninstallString = "…\Update.exe" --uninstall --silent
LINK C:\Users\admin\Desktop\Navo.lnk -> C:\Users\admin\Prog Ă\NaVo\current\navo.exe
LINK C:\Users\admin\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Navo.lnk -> …\current\navo.exe
legacy %LocalAppData%\NavoApp exists=False
UAC prompts: 0
```

Setup's log went to the given file (5449 bytes). Excerpt:

```
[setup:…] Install To: Some("C:\\Users\\admin\\Prog Ă\\NaVo")
[setup:…] Preparing and cleaning installation directory...
[setup:…] Root directory 'C:\Users\admin\Prog Ă\NaVo' writable: true
[setup:…] Extracting stub 'Navo_ExecutionStub.exe' to '"C:\\Users\\admin\\Prog Ă\\NaVo\\Navo.exe"'
[setup:…] Shortcut Previous Locations: ShortcutLocationFlags(0x0) (None)
[setup:…] Creating new shortcut for flag '"C:\\Users\\admin\\Desktop\\Navo.lnk"' (DESKTOP)
[setup:…] Creating new shortcut for flag '"…\\Start Menu\\Programs\\Navo.lnk"' (START_MENU_ROOT)
[setup:…] Running --veloapp-install hook...   Hook executed successfully (took 1078.7719ms)
[setup:…] Writing uninstall registry key...
[setup:…] Installation completed successfully!
```

Setup also echoes its log to stdout. A first P1 attempt installed correctly, but the probe script failed while reading the result: a PowerShell output-capture bug in the helper. That install was uninstalled, the state was checked clean, and P1 was rerun. The evidence above is from the rerun.

## P1h — P1 with the hold: PASS

The probe created `Prog Ă` and `NaVo` with `CreateDirectoryW` and held both through the whole Setup.exe run and the verification. It then wrote the marker through the held root.

```
HOLD C:\Users\admin\Prog Ă      id=…:197B1D00000091000000000000000000 reparse=0
HOLD C:\Users\admin\Prog Ă\NaVo id=…:67831D00000024000000000000000000 reparse=0
RUN NavoApp-win-Setup.exe (0.0.2) --installto "…\Prog Ă\NaVo" --silent --log …\P1h-velopack.log
EXIT 0 after 1.78s
HELD id=…:197B1D… path=C:\Users\admin\Prog Ă
HELD id=…:67831D… path=C:\Users\admin\Prog Ă\NaVo
MARKER written through held root: id=…:0CB21D00000019000000000000000000
```

The listing equals P1's: `current` with 19 files, the `Navo.exe` stub, `packages\` with `.velopack_lock` and the 0.0.2 nupkg, the same `Update.exe` hash, `sq.version` 0.0.2, the same key values and both links. There were no UAC prompts. The log shows no warning or error and ends with `Installation completed successfully!`. R8 (fresh) does not fire.

## P2 — The apply steps 0.0.2 → 0.0.3 on P1's root, Navo closed: PASS

```
STAGED packages\NavoApp-0.0.3-full.nupkg sha256=FA5652381D4D8121…   (via .navo-partial + MoveFileExW)
Squirrel entry: lib/app/Squirrel.exe length=5219840
REPLACED Update.exe sha256=B06A2474…F340C0
RUN "…\Prog Ă\NaVo\Update.exe" apply --package "…\packages\NavoApp-0.0.3-full.nupkg" --silent --norestart
    --root "…\Prog Ă\NaVo" --packageDir "…\Prog Ă\NaVo\packages" --log C:\winvm\installer-w1\logs\P2-velopack.log
  SEEN +1.7s "…\current\navo.exe" --veloapp-updated 0.0.3
EXIT 0 after 1.78s
```

| Check | Result |
| --- | --- |
| exit 0 | True |
| `sq.version` 0.0.3 | True |
| marker: same file id and text | True (`…:1BB11D00000022…` before and after) |
| root file id unchanged | True (`…:26781D0000002F…`) |
| `Update.exe` is the extracted one | True (hash). Its file id changed from `…:B57A1D000000DB02…` to `…:EEB11D00000019…`, so the rename happened. |
| HKCU `DisplayVersion` 0.0.3, `InstallLocation` unchanged | True / True |
| no UAC prompt | True (runner: `consent_prompts=0`) |
| Navo does not start | True (only the hook processes; no `navo.exe` running afterwards) |
| no `%LocalAppData%\NavoApp` | True |
| `Update.exe`'s log lands in `<file>` | True (7259 bytes) |

The hooks, from the apply log (Navo's own log has no hook lines; Navo registers only the uninstall callback):

```
Root Directory: Some("C:\\Users\\admin\\Prog Ă\\NaVo")
Context: FromSpecifiedRootDir("…\\NaVo", Some("…\\NaVo\\packages"))
Restart: false      Silent: true
Running --veloapp-obsolete hook...  Waiting 15000ms…  Hook executed successfully (took 18.913ms)
Inspected 106 running processes in 11ms, 1 matched directory / Skipping killing self: …\Update.exe
Backing up current dir to "…\\packages\\VelopackTemp\\tmp_rM2G5kbsTTWzTVEn"
Replacing current dir with "…\\packages\\VelopackTemp\\tmp_ZlMQy71kVniDVYNe"
Writing uninstall registry key...
Running --veloapp-updated hook...   Hook executed successfully (took 1293.591ms)
Shortcut Previous Locations: DESKTOP | START_MENU_ROOT (Some("0.0.2")) / Next: DESKTOP | START_MENU_ROOT ("0.0.3")
Updating existing shortcut '"C:\\Users\\admin\\Desktop\\Navo.lnk"' / '"…\\Programs\\Navo.lnk"'
Update.exe is already in the correct location
Extracting stub 'Navo_ExecutionStub.exe' to '"…\\NaVo\\Navo.exe"'
Package version 0.0.3 applied successfully.
```

`packages\` before: `.velopack_lock`, `NavoApp-0.0.2-full.nupkg`.

`packages\` after: `.velopack_lock`, `NavoApp-0.0.2-full.nupkg` (`DEFCF5C9…`), `NavoApp-0.0.3-full.nupkg` (`FA565238…`) and an empty `VelopackTemp\`.

## P3h — P2 with the hold on the existing root: PASS

This is P2's script on P1h's root, with `Open-Hold` on the root from before the package step until after verification.

```
HOLD C:\Users\admin\Prog Ă\NaVo id=…:67831D00000024… reparse=0
RUN …\Update.exe apply … (IW3 array, --log …\P3h-velopack.log)
EXIT 0 after 1.77s
HELD id=…:67831D00000024… reparse=0 path=C:\Users\admin\Prog Ă\NaVo
CHECK exit0 / sq_003 / marker_same / root_id_same / update_is_extracted / displayversion_003 /
      installlocation_same / navo_not_started / no_legacy_root / log_in_file = all True
held root id equals before=True;   UAC prompts: 0
```

The log matches P2 line for line: obsolete hook 20.4 ms, updated hook 1264.3 ms, "Package version 0.0.3 applied successfully". `packages\` after is the same as P2's. R8 (upgrade) does not fire: apply renames only `current\` and works while the root is held.

## P2u — Navo's own update 0.0.2 → 0.0.3 from a fresh P1 root: FAIL (R9 stop)

Command: a fresh P1 install (Setup.exe 0.0.2, exit 0) and the test marker by path. Then:

1. Navo started from `current\navo.exe` with a throwaway `--profile` (updates on) and `NAVO_UPDATE_FEED_DIR=<feed holding NavoApp-0.0.3-full.nupkg + releases.win.json>`.
2. Navo was polled with `navo.exe --update-status` until `staged_version` was 0.0.3.
3. Navo was closed, then started again with the same profile, and applied the update at start.

```
status: {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
SEEN "…\Update.exe" apply --package "…\packages\NavoApp-0.0.3-full.nupkg" --waitPid 1796 --root "…\NaVo"
     --packageDir "…\NaVo\packages" -- --profile … --left …
applied=True; Navo restarted (Restart: true), then closed by the probe
```

| Check | Result |
| --- | --- |
| no `%LocalAppData%\NavoApp` | True |
| marker survives (same id and text) | True |
| root file id unchanged | True |
| UAC prompts | 0 |
| `Update.exe` hash equals P2's | True: `B06A2474…F340C0`. This is trivially equal, because both packs carry the same `Update.exe`. |
| `packages\` equals P2's | **False** |

`packages\` compared:

| Entry | P2 (wizard steps) | P2u (library) |
| --- | --- | --- |
| `.velopack_lock` | yes | yes |
| `NavoApp-0.0.2-full.nupkg` (39 MB) | **kept** | **deleted at download** |
| `NavoApp-0.0.3-full.nupkg` | yes (`FA565238…`) | yes (`FA565238…`) |
| `.betaId` (36 B) | **absent** | **created at the update check** (12:35:04.664Z) |
| `VelopackTemp\` (empty) | yes | yes |

Staged state, while Navo was still running and before apply: `packages\` held `.betaId`, `.velopack_lock` and `NavoApp-0.0.3-full.nupkg`. The 0.0.2 nupkg was already gone at download time.

`Update.exe` handling also differs. The library rewrites `Update.exe` in place: its file id stayed `…:EC871D0000006C…`, created 12:34:41.748Z and modified 12:35:04.730Z, the moment of the download. The wizard's steps replace it with a new file (P2: new file id). The content is the same, and IW4 check 2 compares hashes, so this difference does not change a verdict.

Source, for the cause (velopack crate 1.2.161, `src/manager.rs`):
- 418–421 and 437–438 collect every `packages\*.nupkg` and `*.partial` before the download.
- 463–475 extract `Squirrel.exe` over `get_update_path()`.
- 478–481 delete the collected files ("Deleting up old package").

IW8b step 4 has no counterpart to that deletion.

The library's apply wrote to Velopack's default log, `%LocalAppData%\velopack\velopack_NavoApp.log`, because it passes no `--log`:

```
Silent: false   Restart: true   Wait: WaitPid(1796)
[WARN] Failed to wait for process (1796) to exit (Access is denied. (os error -2147024891)). Continuing...
Applying package 0.0.3 to current: 0.0.2
Running --veloapp-obsolete hook... (took 19.2709ms)
Running --veloapp-updated hook...  (took 1335.1831ms)
Updating existing shortcut … Desktop\Navo.lnk / … Programs\Navo.lnk
Update.exe is already in the correct location
Package version 0.0.3 applied successfully.
```

Verdict: the wizard's steps do not leave the state the library leaves. The difference is the kept 0.0.2 nupkg and the missing `.betaId`. The stop rule "P2u leaves another state than P2 (R9)" fires. The run stopped here; cleanup is below.

## P3, P3d, P3s, P4, P5, P6, P7, P8, P9, P10: SKIPPED

None of these was run. W1's Check line stops the run when P2u leaves a different state than P2, and that fired. They give no evidence either way. After the plan changes, they need a new W1 run (or a W1 continuation).

The future run needs these preparations. All are cheap and reversible:
- P9: no second admin account exists. A temporary local admin can be created and its profile deleted afterwards.
- P7: a 50 MB VHD can be created with diskpart.
- P8 and P10: they need UAC answers. Killing `consent.exe` from the elevated runner declines a prompt. `ConsentPromptBehaviorAdmin=0`, set temporarily, accepts one.

The packs (0.0.2, 0.0.3 and 0.0.4) and the probe scripts stay in `C:\winvm\installer-w1\` for that run.

## Cleanup

The final state was checked through the elevated SSH session and through the limited user:

- `HKU\S-1-5-21-…-1000\…\Uninstall\NavoApp`: absent.
- `Desktop\Navo.lnk`, `Start Menu\Programs\Navo.lnk`, `Mine.lnk`: absent. No `.lnk` points at a `current\navo.exe`.
- `C:\Users\admin\Prog Ă`: removed; it was empty after each uninstall. No `%LocalAppData%\NavoApp`. No Navo folder under Program Files. No `Navo Index` service.
- Processes: no `navo`, `Update` or `*Setup*`. Scheduled tasks named `NavoW1-*`: 0 (each one was removed when its run ended).
- `C:\winvm\installer-w1\scratch`: emptied. `C:\winvm\installer-w1` keeps the packs, the logs and the probe scripts (398 MB) for a rerun.
- Every install was removed with `"<root>\Update.exe" --uninstall --silent`, which is the `QuietUninstallString`. Each removal was checked clean before the next probe.

Side effects outside the test root, all from Navo's or Velopack's own behaviour:
- `C:\Users\admin\AppData\Local\Navo\logs\navo.log` grew from 21178 to 21610 bytes. Navo's uninstall callback appended four `"uninstall: no index service to remove"` lines, one per uninstall. Its data folder comes from `%LocalAppData%` whatever the install root is. No other file in that folder changed.
- `%LocalAppData%\velopack\velopack_NavoApp.log` grew from 159346 to 167547 bytes because of P2u's library apply. `velopack.log` is unchanged.

## Plan impact

Confirmed:
- **IW3, both arrays.** Setup.exe parsed `--installto`, `--silent` and `--log` (`Install To: Some(…)`, `Silent: true`, and the log in the given file).
  - For apply, `--root` and `--packageDir` were parsed: `Root Directory: Some(…)`, `Context: FromSpecifiedRootDir(…, Some(…\packages))`. `--norestart` gave `Restart: false`, `--silent` gave `Silent: true`, and `--log` was honoured.
  - Both Setup.exe and apply also echo the log to stdout.
- **IW4.** Both Setup.exe and apply exit 0 on success.
  - `InstallLocation` is the exact `--installto` text, including U+0102 and the case of `NaVo` (case-sensitive equality is True).
  - Apply rewrites the key's `DisplayVersion` and keeps `InstallLocation`.
- **IW7.** An empty, pre-created root is accepted and keeps its file id. The marker written after verification survives apply (P2, P3h) and the library's update (P2u).
- **IW7a and R8.** The exact hold works with Setup.exe on an empty held root (P1h) and with `Update.exe apply` on a held root (P3h). Writing the marker through the held handle (`NtCreateFile` with `RootDirectory`) works with the hold's access mask. R8 does not fire.
- **IW8b steps 5–6 and IW11.** Apply keeps root files and updates only existing shortcuts: "Shortcut Previous Locations" equals "Next", and "Updating existing shortcut". This is only partial evidence for R4, because P4 did not run.
  - Apply does not touch `Update.exe` ("already in the correct location").
  - Apply re-extracts the root `Navo.exe` stub.
- **IW12.** With `--norestart`, Navo does not start; only `--veloapp-*` hook processes appear. The library's own apply restarts Navo (`Restart: true`), as expected.
- **Hook timings.** The hooks finish well within Velopack's limits: install 1.08 s (limit 30 s), obsolete about 20 ms, updated 1.26–1.34 s (limit 15 s). Apply's force-stop scans matched only `Update.exe` itself.
- **IW13 and W0 Q8.** A writable per-user root raises no UAC prompt for either path.

Contradicted:
- **IW8b step 4 against R9 (stop).** The library deletes every other `*.nupkg` and `*.partial` in `packages\` when it downloads (`manager.rs:418-438, 478-481`). The wizard's steps keep the old full package, so a manual upgrade leaves about 39 MB that the library would not leave.
  - The library also creates `packages\.betaId` during its check. The wizard never runs the library, so it creates none.
  - Options for the orchestrator:
    - (a) Add to IW8b step 4: after the rename, delete the other `*.nupkg` and `*.partial` files in `packages\` by handle, as the library does. This makes W8u and W8b test it.
    - (b) Accept the leftover and document it. The next in-app download deletes it anyway (`manager.rs:437-438`).
    - In both cases, decide whether `.betaId` matters. It is the library's staged-rollout id, and the next in-app check creates it.
  - With (a), the difference that remains is `.betaId` alone. R9 then needs restating to allow that difference, or the wizard creates it.
- **IW8b step 5 against the library.** The wizard replaces `Update.exe` with `MoveFileExW`, which gives a new file id. The library overwrites it in place, which keeps the id. IW4 check 2 compares SHA-256, so this changes no verdict. A future check that compares `Update.exe` file ids would see the difference.

New observations for discovery (IW8) and verification:
- A Velopack root holds a root-level stub `Navo.exe` (480256 bytes, re-extracted by every apply) and `packages\.velopack_lock`.
- After any apply, the root holds an empty `packages\VelopackTemp\`. After the library's own update, it also holds `packages\.betaId`.
- Because all packs share one `Update.exe` binary, R9's hash comparison cannot fail between two packs from one vpk version. Only the file id and modification time show what happened.
- The library's apply logged `Failed to wait for process (…) to exit (Access is denied…). Continuing...` for the Navo that started it. This is outside W1's scope. It is relevant to IW9's "running" race, because apply then relies on its force-stop.

Not covered (SKIPPED): IW8's Newer guard against apply downgrades (P3d), R2's Setup.exe-over-root evidence (P3s), IW9 and IW8b step 7 force-stops with Navo running (P3), R4 and IW11 deleted-link checks (P4), IW20 and uninstall scope (P5, P10), the missing parent chain (P6), W0 Q8, Q11 and Q13 under failure (P7), elevation (P8, P9).

## Run 2 (plan 1c8b1182)

Date: 2026-11-09, 14:44–15:28 Europe/Bucharest. Plan: main `1c8b1182`. Same VM, packs (0.0.2, 0.0.3, 0.0.4), test root `C:\Users\admin\Prog Ă\NaVo` and driver as run 1. Every VM step held the shared VM lock. No cargo build.

**Result: no stop rule fired.** P2 passes with the corrected step 4, and P2u leaves the same R9 state. Every probe ran.

| Probe | Verdict |
| --- | --- |
| P1 (rerun, P2's base) | PASS |
| P2 | PASS |
| P2u | PASS: R9 compared state equal |
| P1h, P3h (rerun with the new step 4) | PASS |
| P3 | RECORDED |
| P3d | RECORDED |
| P3s | RECORDED |
| P4 | PASS (Desktop and Start menu) |
| P5 | PASS |
| P6 | PASS |
| P7 | RECORDED |
| P8 | RECORDED |
| P9 | RECORDED |
| P10 | RECORDED (accepted, declined, left open) |

### Changes to the driver

- `lib.ps1` `Invoke-ApplyPrep` now does IW8b step 4 as corrected. After the rename, it deletes every other `*.nupkg` and `*.partial` file in `packages\` through its own handle. The handle is `CreateFileW(DELETE | FILE_READ_ATTRIBUTES, FILE_FLAG_OPEN_REPARSE_POINT)` followed by `SetFileInformationByHandle(FileDispositionInfo)`. It touches no other file.
- `Save-R9State` writes R9's compared state to JSON: the `*.nupkg` set with hashes, `Update.exe`'s SHA-256, `sq.version`, whether the marker survived, and every HKCU value. `CompareR9.ps1` compares P2 with P2u field by field.
- `Assert-PreClean` runs at the start of every standalone probe. It throws if any of these is present: the uninstall key, a Navo link, `%LocalAppData%\NavoApp`, a test tree, a navo/Update/Setup process, the `Navo Index` service, or `Program Files\Navo W1`. `Clean.ps1` runs between probes. It now also empties `scratch\`; run 1's `-LiteralPath …\*` never matched anything. It also removes a `%LocalAppData%\NavoApp` fallback folder that a probe created, after listing it.
- **UAC answers are real key presses.** The host sends keys through QEMU's QMP `send-key`, which reaches the secure desktop: Esc means No and Alt+Y means Yes. The runner still counts `consent.exe` starts (`Observe-Consent` style). Killing `consent.exe` turned out **not** to work as a decline. In the first P7c attempt, `Update.exe` hung in its elevated relaunch for more than 300 s, until the cleanup uninstall force-stopped it. That attempt is discarded.
- Probe bugs found and fixed during the run, with no product impact:
  - A `$during` script block called `Start-Navo $exe`. PowerShell's dynamic scoping resolved `$exe` to `Invoke-Watched`'s own parameter, which is `Update.exe`. The first P3b/P3c results were therefore invalid and were rerun.
  - P3c first matched old text in an appended apply log; the log is now deleted first.
  - `WScript.Shell` stores a link target as ANSI, so `Ă` became `A`. P5's `Mine.lnk` therefore pointed outside the root on the first attempt. P5 now rewrites the target through `Shell.Application`, which keeps Unicode, and checks it.

### P1 and P2: PASS

P1 rerun: exit 0 after 2.01 s, the root id stayed the same, `InstallLocation` equals `C:\Users\admin\Prog U+0102\NaVo` exactly, and the log went to the given file.

P2 on P1's root (0.0.2 with the test marker → 0.0.3, Navo closed):

```
STAGED packages\NavoApp-0.0.3-full.nupkg sha256=FA5652381D4D8121…
DELETED packages\NavoApp-0.0.2-full.nupkg id=…:E2B51D00000032… (by handle)
packages after step 4: .velopack_lock, NavoApp-0.0.3-full.nupkg
REPLACED Update.exe sha256=B06A2474…F340C0
RUN …\Update.exe apply --package …\packages\NavoApp-0.0.3-full.nupkg --silent --norestart --root … --packageDir …\packages --log …\P2-velopack.log
EXIT 0 after 1.72s       (obsolete hook 19.98 ms, updated hook 1242.75 ms; force-stop matched only Update.exe)
CHECK exit0 sq_003 marker_same root_id_same update_is_extracted displayversion_003 installlocation_same
      navo_not_started no_legacy_root log_in_file packages_only_new = all True;   runner: consent_prompts=0
packages\ after apply: .velopack_lock, NavoApp-0.0.3-full.nupkg, VelopackTemp\ (empty)
```

### P2u: PASS (R9)

This was a fresh P1 root with the test marker, updated by Navo's own update to 0.0.3 through `NAVO_UPDATE_FEED_DIR`. Navo staged the package (status `staged_version 0.0.3, ready`), closed, and applied it at the next start (`Update.exe apply … --waitPid …`). There were no UAC prompts.

`CompareR9.ps1`, comparing exactly what R9 lists:

| Field | P2 (wizard steps) | P2u (library) | Equal |
| --- | --- | --- | --- |
| `*.nupkg` in `packages\` | `NavoApp-0.0.3-full.nupkg FA565238…` | same | yes |
| `Update.exe` SHA-256 | `B06A2474…F340C0` | same | yes |
| `sq.version` | `NavoApp 0.0.3` | same | yes |
| marker survived (same id and text) | true | true | yes |
| HKCU `DisplayIcon`, `DisplayName`, `DisplayVersion` 0.0.3, `EstimatedSize` 133914, `InstallDate`, `InstallLocation`, `Language`, `NoModify`, `NoRepair`, `Publisher`, `QuietUninstallString`, `UninstallString` | values | identical, compared case-sensitively with U+0102 shown | yes (all 12) |

`VERDICT R9 compared state equal=True`.

Not compared, as R9 says:
- P2u also has `packages\.betaId`.
- `Update.exe`'s file id: the library kept the installed file's id (`…:D6B61D…30`), while the wizard's rename gives a new one.

### P1h and P3h, rerun with the new step 4: PASS

P1h: exit 0. Both holds kept their ids, and the marker was written through the held root.

P3h: the held root kept its id (`…:3FBD1D…1F`). Step 4 deleted the 0.0.2 nupkg by handle while the root was held, and all ten P2 checks passed. The hold does not block the deletion inside `packages\`, so R8 still does not fire.

### P3: RECORDED (apply with Navo running)

- **P3a: Navo running before the apply steps (0.0.2 → 0.0.3).**
  - Navo pid 1748 and its `--pdf-helper` child, pid 2300, were running.
  - Apply ran the obsolete hook (19 ms). Its force-stop then killed both: `Killing process: …\current\navo.exe (1748)`, then `(2300)`. Both had exit code 1.
  - Apply then replaced `current\` and ran the updated hook (1.28 s).
  - Exit 0, `sq.version` 0.0.3, the marker unchanged. No Navo was running afterwards.
- **P3b: Navo started 0.26 s into the apply run (0.0.3 → 0.0.4).**
  - The first force-stop scan after the obsolete hook killed it (`2 matched directory`, `Killing … (1640)`). Exit 0, 0.0.4.
- **P3c: Navo started while the updated hook ran, after `current\` was replaced (+1.63 s).**
  - The force-stop scan after the updated hook killed it (`Killing … (1692)`). Exit 0.
  - The killed Navo had already started writing its profile (`data\.running.tmp-1692-0`, `logs\navo.log`).
- **Diagnostic finding.** A Navo that starts while a newer full package sits in `packages\` applies it at start, through `updater::apply::at_start` and the library. The wizard stages the package before apply runs (IW8b step 4). A Navo started in that window would therefore start its own apply. In P3b and P3e it was killed first.

Summary: every Navo that ran before the last force-stop check was killed, and apply still exited 0. IW9's refusal, plus its recheck just before apply, are the only protections. A Navo started after apply's last check (not hit here) would survive.

### P3d: RECORDED (0.0.2 package over 0.0.3)

- Step 4 deleted the newer `NavoApp-0.0.3-full.nupkg` by handle.
- Apply then reported `Applying package 0.0.2 to current: 0.0.3` and `Package version 0.0.2 applied successfully`, with exit 0.
- Afterwards `sq.version` and `DisplayVersion` were 0.0.2, and the marker was unchanged.

**Apply downgrades silently.** IW8's Newer check remains the only guard, as the plan says.

### P3s: RECORDED (Setup.exe 0.0.3 over a 0.0.2 root, R2 evidence)

Unheld, with Navo running and the Desktop link deleted beforehand:

```
User chose to overwrite existing installation.
Killing process: …\current\navo.exe (2772) / (1500)
Renaming existing directory to '…\Prog Ă\NaVo.lDOUeEBO7UIvpoLK' to allow rollback...
Creating new shortcut for flag '…\Desktop\Navo.lnk' (DESKTOP)          ← the deleted link is re-created
Running --veloapp-install hook... (1181 ms)
Installation completed successfully!   Removing rollback directory...
EXIT 0;  root id changed (…:5EC31D…2E → …:9EC31D…1A);  marker absent;  packages\: .velopack_lock, NavoApp-0.0.3-full.nupkg;  no sibling left
```

Held: the parent and root held with IW7a's handle.

```
Renaming existing directory to '…\NaVo.G3G4vZrFPdfBLNu4' to allow rollback...
Retrying operation in 1000ms... (os error 32) ×4
ERROR Failed to remove existing application directory, please close the application and try running the installer again…
EXIT 1 after 4.04 s;  root id same;  marker same;  sq.version 0.0.2;  links untouched
```

Both cases match W0 Q2 and Q11.

### P4: PASS (a deleted link stays deleted)

| Case | After the apply steps (0.0.2 → 0.0.3) | After Navo's own update (0.0.3 → 0.0.4) |
| --- | --- | --- |
| Desktop link deleted | absent; the Start menu link was updated (`Updating existing shortcut … START_MENU`) | absent; the Start menu link was updated again |
| Start menu link deleted | absent; the Desktop link was updated | absent; the Desktop link was updated again |

Both paths log `Shortcut Previous Locations … DESKTOP | START_MENU_ROOT`. Each updates only the links that its search finds. Neither logs `Creating new shortcut`. IW11's stop rule does not fire.

### P5: PASS (uninstall through `UninstallString`)

Setup:
- the test root, 0.0.2, with `NaVo\user-file.txt` inside;
- `Prog Ă\beside.txt` beside the root;
- a user-made `Desktop\Mine.lnk` whose target is exactly `…\Prog U+0102\NaVo\current\navo.exe`.

Uninstall command: `"…\Update.exe" --uninstall`, without `--silent`.

```
Running --veloapp-uninstall hook... (37 ms)   (Navo log: "uninstall: no index service to remove")
Selected shortcut … Desktop\Mine.lnk / Desktop\Navo.lnk / Programs\Navo.lnk  → Removing shortcut ×3
Removing directory '…\NaVo'; Removing uninstall registry keys...; Finished successfully.
```

- **The interactive uninstall shows an "Uninstall Complete" box (title "Navo Uninstall").**
  - While the box is open, the root still holds `Update.exe` and `user-file.txt`. The key and all links are already gone.
  - Only after OK does it run `cmd.exe /C choice … /T 3 & rmdir /s /q "<root>"`.
  - The probe pressed Enter on the box. In run 2's first P5 attempt nobody answered it, so `Update.exe` waited for more than 120 s with the root not yet removed.
- Checks: exit 0, root gone, the file inside gone, `beside.txt` kept, key gone, Navo links gone, user data kept. The only change in `%LocalAppData%\Navo` was one appended `navo.log` line.
- **`Mine.lnk` was deleted.** This confirms W0 Q6 for a user-made link whose target is stored as Unicode.

### P6: PASS (a missing parent chain)

- **P6a.** Setup.exe ran with `--installto C:\Users\admin\W1 Ă\x y\NaVo`, where neither `W1 Ă` nor `x y` existed. It created both parents and the root and exited 0. `InstallLocation` is the exact text (W0 Q1).
- **P6b.** This is the wizard's way. `New-Chain` created `W1 Ă\p q`, then `r`, then `NaVo`, and all three were held with IW7a's handle. Setup.exe exited 0, all three holds kept their ids, and the marker was written through the held root.
- In both cases uninstall removes only the root. The parents the probe created (`x y`, `p q\r`) stay behind.

### P7: RECORDED

- **P7a: full disk.** The root was `W:\Apps\NaVo` on an NTFS VHD of 50 MB with 38.9 MB free.
  - Setup.exe refused before writing anything, exit 1 after 0.02 s: `Navo requires at least 232.24 MB disk space to be installed. There is only 38.94 MB available.`
  - The root was left empty. There was no key and no links.
  - With enough space, Setup.exe logs `this package requires 232.24 MB`. A disk that fills during the write could not be produced, because of this upfront check.
- **P7b: Setup.exe killed 0.33 s into the run, during `Extracting 25 app files to …\current`.**
  - Exit -1 (killed). Seven files stayed in the root: `Update.exe`, the `Navo.exe` stub, `packages\` with `.velopack_lock` and the 0.0.2 nupkg, and `current\` with 3 files including `sq.version` 0.0.2.
  - There was no key, no links and no leftover process. The root was then deleted by hand. This is W0 Q11's "no rollback" case.
- **P7c: the apply steps on a root the user cannot write.** The deny ACE `(WD,AD)` for the user was set on the root after step 5. A write probe was refused.
  - `Update.exe apply` logged `Root directory … writable: false` and `Using fallback directory: C:\Users\admin\AppData\Local\NavoApp`. It then logged `Re-launching as administrator to update …`.
  - There was **one UAC prompt**, declined with Esc. Result: `Apply error: … The operation was canceled by the user. (os error -2147023673)`, exit 1 after 3.66 s.
  - `sq.version` stayed 0.0.2 and the marker was unchanged.
  - **The fallback folder `%LocalAppData%\NavoApp` (`Update.exe` 5219840 B, an empty `packages\`) was created before the prompt and stays after the decline.**

### P8: RECORDED (elevated, Program Files)

- **P8a: elevated token (`-RunLevel Highest`), `Setup.exe --installto "C:\Program Files\Navo W1"`.**
  - 0 prompts, because the token was already elevated. Exit 0, `writable: true`, and `packages\` in the root.
  - The key went to the same user's HKCU (`HKU\…-1000`). The links went to that user's Desktop and Start menu.
- **P8b: the limited user, Navo's own update to 0.0.3, prompt declined with Esc.**
  - The library staged the package in the fallback: `%LocalAppData%\NavoApp\packages\` held `NavoApp-0.0.3-full.nupkg`, `.betaId` and `.velopack_lock`, and `Update.exe` was copied there.
  - At the next start, `…\AppData\Local\NavoApp\Update.exe apply … --root "C:\Program Files\Navo W1" --packageDir …\NavoApp\packages` asked for elevation. That was **one prompt**, and it was cancelled by the user.
  - Nothing was applied: 0.0.2 remained.
- **P8c: the same, prompt accepted with Alt+Y.**
  - **One prompt**, raised by Navo's at-start apply. The elevated `Update.exe` applied 0.0.3 with `VelopackTemp` inside the fallback.
  - It logged `Running from non-default location. Attempting to update default Update.exe at: "C:\Program Files\Navo W1\Update.exe"`, then `Successfully updated`.
  - Navo restarted unelevated. The links were updated.
  - The root's own `packages\` still holds the stale `NavoApp-0.0.2-full.nupkg`.
- **Cleanup: elevated `--uninstall --silent`.** It removed the root, the key and the links. **The fallback `%LocalAppData%\NavoApp` (`Update.exe` plus a 39 MB nupkg) stayed** and was removed by `Clean.ps1`.

### P9: RECORDED (a second admin, elevated)

A temporary local admin, `w1admin2`, was created. Its random password was never printed. It ran Setup.exe 0.0.2 elevated (`RunLevel Highest`, password logon, session 0) with `--installto C:\Users\admin\Prog Ă\NaVo`.

```
user=WINVM\w1admin2 sid=…-1001 elevated=True;  EXIT 0
key in second admin's hive (HKU\…-1001): True, InstallLocation=C:\Users\admin\Prog U+0102\NaVo
key in admin's hive (HKU\…-1000): False
links: C:\Users\w1admin2\Desktop\Navo.lnk and …\w1admin2\…\Programs\Navo.lnk = True; admin's Desktop/Start menu = False
root owner = BUILTIN\Administrators;  second admin's %LocalAppData%\NavoApp: absent
uninstall as w1admin2 (--uninstall --silent): exit 0, key, root and its links gone
```

The entries go to the profile of the account that runs Setup.exe, not to the user who owns the folder. Admin's Apps & Features would show nothing. This supports IW13's refusal to run elevated.

The account was deleted. **Its profile folder `C:\Users\w1admin2` (141 items) could not be deleted**: Windows still has the profile hive loaded (`Loaded=True`, `Remove-CimInstance` and `reg unload` refused), and no process of that user remains. It goes after the next VM restart:

```
Get-CimInstance Win32_UserProfile | Where-Object LocalPath -like '*w1admin2*' | Remove-CimInstance
```

### P10: RECORDED (uninstall with the index service)

Each case:
1. a fresh 0.0.2 install;
2. elevated `current\navo.exe --index-service install` (exit 0, `Navo Index` Running, image `C:\Program Files\Navo\Index\navo-index.exe`);
3. a limited `QuietUninstallString` (`--uninstall --silent`), with the hook's prompt answered by a key.

| Case | Prompts | Navo log (hook) | Uninstall | Service after |
| --- | --- | --- | --- | --- |
| accepted (Alt+Y after 5 s) | 1 | `uninstall: index service removed` (t=4881 ms) | exit 0, root and key gone | absent; `Program Files\Navo` gone |
| declined (Esc after 5 s) | 1 | `uninstall: index service left in place`, reason `index service elevation was cancelled` (t=4025 ms) | exit 0, root and key gone | Running; `Program Files\Navo` present |
| left open (Esc after about 33 s) | 1 | `uninstall: index service left in place`, reason `no answer within 25 s` (t=25001 ms) | `Update.exe` exit 0 after 25.27 s; root and key gone | Running; `Program Files\Navo` present |

- In the left-open case the prompt was still open when uninstall finished. It was closed with Esc later. Accepting it at that point was not tried.
- After each decline, the leftover service was removed with an elevated `navo.exe --index-service remove` from the tester build (exit 0). The service and `Program Files\Navo` were then absent.

### Cleanup (run 2)

These were checked with `Final.ps1` (limited) and the elevated SSH session:

- No `Uninstall\NavoApp` key. No Navo, `Mine.lnk` or `current\navo.exe` link. No `%LocalAppData%\NavoApp`.
- No `Prog Ă` or `W1 Ă` tree. No `Program Files\Navo W1` and no `Program Files\Navo`. No `Navo Index` service.
- No navo, Setup or Navo `Update.exe` process. No `NavoW1-*` scheduled task.
- `scratch\` is empty. W: and the VHD file are gone.
- `ConsentPromptBehaviorAdmin` is still 5; it was never changed.
- The `w1admin2` account is deleted. Its profile folder stays until a restart; see P9.
- Side effects:
  - `C:\Users\admin\AppData\Local\Navo\logs\navo.log` grew from 21610 to 24397 bytes. These are Navo's own uninstall-callback lines: 22× `no index service to remove`, 1× `index service removed`, 2× `index service left in place`. No other file in that folder changed (P0 and Final listings are equal).
  - `%LocalAppData%\velopack\velopack_NavoApp.log` grew from 167547 to 233295 bytes, from runs that pass no `--log`.
- `C:\winvm\installer-w1` keeps the packs, run 2's probes (run 1's copies are in `probes\run1\`) and the logs (run 1's are in `logs\run1\`).

## Plan impact (after run 2)

Confirmed:
- **IW8b step 4 as corrected, and R9.** The wizard's steps now leave the library's state exactly: the same nupkg set, `Update.exe` hash, `sq.version`, marker and all 12 HKCU values.
  - Deleting by handle works on a held root (P3h).
  - The library also deletes old packages at every app start (`velopack` `app.rs:194`, `228-243`). Any leftover would go anyway.
- **IW8b step 8's extras.** The `Navo.exe` stub, `packages\.velopack_lock` and an empty `packages\VelopackTemp\` were present after every apply.
- **IW8, the Newer guard (P3d).** Apply downgrades silently with exit 0, so the guard is required.
- **IW9 (P3).** Apply's force-stops kill a running Navo, and any Navo started before apply's last check, and apply still exits 0.
- **R2 (P3s).** Matches W0 Q2 and Q11:
  - unheld: kill, rename, re-created links, marker lost, exit 0;
  - held: exit 1 after 4 s of retries, with the root intact.
- **IW11 and R4 (P4).** A deleted link stays deleted after both the apply steps and Navo's own update.
- **IW7 and IW7a (P6).** Setup.exe creates missing parents itself, and it also works under a held three-level chain.
- **IW20 (P5, P10).** Uninstall removes the root with user files inside, every `.lnk` that points into it (including a user-made one), and the key. It keeps files beside the root and Navo's data.
  - The service hook behaves as IW20 says: removed on Yes; left in place on No, or after 25 s without an answer.
- **IW13 (P9).** An install elevated by another admin lands in that admin's hive and profile.
- **W0 Q8 and Q13 (P7c, P8).** A root the user cannot write makes apply relaunch itself elevated with one prompt, even with `--silent`.

New findings for the plan:
1. **IW7's free-space rule is lower than Velopack's.**
   - Setup.exe for this 39 MB package demands 232.24 MB free and exits 1 before writing anything if that is missing.
   - IW7 asks for 4 × the payload size. That is about 188 MB for the 47 MB Setup.exe, or 158 MB for the nupkg.
   - A folder can therefore pass the wizard's check and fail in Setup.exe. The result is safe (an empty root, IW7a cleanup), but it is a confusing failure.
   - Suggestion: derive the rule from Velopack's figure, or use at least 6 × the nupkg size. Show Velopack's message on the Failed page.
2. **Velopack's fallback folder is the legacy root path.**
   - In two cases `Update.exe` or the library creates `%LocalAppData%\NavoApp\` (holding `Update.exe` and `packages\`, sometimes with a 39 MB nupkg) for a root the user cannot write:
     - before asking for UAC, even when the prompt is declined (P7c);
     - for every update of a Program Files install (P8).
   - Uninstall does not remove it.
   - IW8 treats that path as the legacy install. Discovery must not count such a folder, which has no `current\sq.version`, as an install, or "the legacy folder found" would block fresh installs.
   - Suggestion: IW8 should require the legacy folder to be valid by the same rules before it counts as found. An invalid legacy folder should be ignored, or be Ambiguous only when the key also names it. W7c's T3 table should add this case.
3. **A Navo that starts during IW8b's window applies the staged package itself.** The window runs from step 4 (the package staged) to apply's last force-stop check. Navo's `updater::apply::at_start` sees the newer full package and starts its own `Update.exe apply`.
   - In P3 such starts were killed by apply's force-stop.
   - A start between step 4 and step 6's spawn is the same race IW9 accepts. In that case, the two applies compete for `packages\.velopack_lock`.
   - Worth one line in IW9 or IW8b: a Navo started in that window may apply the update itself, and the wizard's apply then fails or finds the version already current. It must report that case, not "failed".
4. **An interactive `UninstallString` ends with an "Uninstall Complete" box.** The root, including user files, is removed only after the box is closed (P5). This is for the user guide only; there is no code impact.
5. **Uninstall keeps parents the wizard created** (for example `Prog Ă` in P6). Velopack removes only the root. The plan does not promise otherwise; the user guide could say so.
6. **For future VM checks: UAC answers.** QMP keys reach the secure desktop: Esc declines and Alt+Y accepts. That makes accept, decline and the 30-second case automatable without an owner check. Killing `consent.exe` is not a valid decline (P7c's first attempt hung).

Contradictions with the plan: none. Run 1's IW8b-step-4 finding is fixed, and R9 now holds.
