# P29 · Crash report

Status: C1, C2 merged; C3 ready; C4, C5 added 2026-10-29 (blocked) · Repo: `<repo>` · Branch: `p29-cN` per atom · Started: 2026-10-19

## Goal

When Navo crashes, the evidence is kept: a redacted crash record is written at the moment of the panic, and the next start says "Navo closed unexpectedly" with one action to save a diagnostic report that includes it. Architecture: "Crashes: a panic hook writes a redacted crash report"; diagnostics table row "Panics and crashes: message, thread, backtrace, the last events before the crash". Today nothing catches a panic.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Panic hook.** Installed in navo-app right after logging, for window runs (not `--version`, `--diag-report`, or a second launch that hands off). It chains to the previous hook. On any panic, on any thread, it writes `<data>/logs/crash-<UTC yyyymmdd-hhmmss>-<pid>.json`: version, commit, thread name, panic location (file:line as compiled), the panic message passed through navo-diag's text redactor, `std::backtrace::Backtrace::force_capture()` frames as text, and the last 200 events of the event ring (already redacted). Written with the existing atomic write; never panics itself; a failure to write is ignored. At most 5 crash files are kept (oldest removed).
- **D2 — Unclean exit.** A marker `<data>/running` is created after the instance lock is taken and removed on a clean quit (the existing quit path). At the next start, a marker that is still there means the last run ended uncleanly, even without a crash file (a hard kill, power loss).
- **D3 — Next start.** After an unclean exit, the window shows a notice "Navo closed unexpectedly last time." with two actions: "Save report" and "Dismiss" (Esc). Save report writes the normal diagnostic report (as `--diag-report` does) plus every crash file into `navo-report-<yyyymmdd-hhmm>.zip` in the user's Downloads folder (Known Folder; on other systems `~/Downloads`, else the home folder), then opens a new tab there with the cursor on the zip and shows "Saved navo-report-….zip in Downloads". Both actions clear the state, so the notice shows once per crash.
- **D4 — Privacy.** Nothing is sent anywhere. The canary test of navo-diag is extended: a panic message containing the canary user name, path and server name must not appear in the crash file.
- **D5 — Out of scope.** Restoring the session is already automatic; SQLite integrity checks after a crash are a later atom.
- **D6 — Native crashes (D88, 2026-10-29).** The panic hook misses access violations, stack overflows and aborts in native code (GPU driver, shell extensions, PDFium). As Zed does, `crash-handler` =0.8.1 catches them in the app and `minidumper` =0.11.0 writes a minidump from a small monitor process (`navo.exe --crash-monitor`, started lazily on a worker thread after the first frame). Dumps go to `<data>/logs/crash-<UTC yyyymmdd-hhmmss>-<pid>.dmp`, at most 5 kept, and count as a crash for D2 and D3. Nothing is sent anywhere (D4). Minidumps hold raw memory and cannot be redacted, so Save report adds them only per Q1. Kept only if the monitor process adds under 10 ms to warm start on the owner's laptop (D88: measure first).

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | D1, D2, D4, and a `crash::pending(data_dir) -> Option<Pending>` plus `crash::save_report(…) -> Result<PathBuf>` API for the UI | api-builder (Codex sol xhigh), then reviewer (Opus medium) | xhigh | — | unit tests: hook writes a redacted file (canaries absent), chains, keeps 5, never panics on a read-only dir; marker created and removed on clean quit, left on a simulated crash; `pending` reports unclean exit with and without crash files; `save_report` zip contains the report and crash files; gate | done |
| C2 | D3 notice and actions in the window | ui-builder (Opus medium) | medium | C1 | UI tests: notice after an unclean start, none after a clean one, Save report opens the tab on the zip and shows the message, Dismiss and Esc clear it; screenshots light and dark; gate | done |
| C3 | Readable backtraces in release: `debug = "line-tables-only"` in the release profile, `navo.pdb` shipped in the release zip next to the exe, size guard checked | infra-builder (Opus medium) | medium | C2 | Windows CI: a forced panic in the release exe writes frames with Navo function names; zip contains the PDB; gate | ready |
| C4 | D6 crash monitor: `crash-handler` + `minidumper`, pinned, monitor started after the first frame; start-up cost measured | api-builder (Sol 6.1 high or DeepSeek V4.1 Flash), then reviewer (Opus medium) | high | C3 | Windows CI: a test exe that writes through a null pointer leaves one `.dmp` that `minidump-stackwalk` or WinDbg opens; keep-5 test; warm start before and after in the report; gate | blocked |
| C5 | D6 dumps in the D3 notice and in Save report, per Q1 | ui-builder (Sonnet medium) | medium | C4 | UI tests: a pending `.dmp` shows the notice; Save report follows Q1; screenshots light and dark; gate | blocked |

## Done when

- A forced panic in a test build leaves a crash file without personal data, and the next start offers the report; the gate passes.
- A forced native crash on Windows leaves a minidump, and the next start offers the report (C4, C5).

## Questions for the owner

**Q1 — Minidumps in the report.** A minidump holds raw memory: file names, paths and file contents that were open. Should Save report include it?
*Recommended:* the notice gets a check box "Include memory dump (may contain file names and file contents)", off by default. The report zip then holds the dump only when the user ticks it.
*Until answered:* the box exists and is off.

## Log

- 2026-10-19 — Plan written. No panic hook exists; reports are saved only with `--diag-report`.
- 2026-10-19 — C1 and C2 done on `p29-c2`. Opus review: merge after fixes. Codex (sol high) fixed: Save report skips a crash file deleted since start (keep-5 retention could make every save fail); no marker or notice without an instance guard (Linux); tests through the real start order; canaries checked in every zip entry; Windows Downloads assertion. Orchestrator: gallery hints hide the crash-notice key unless the page shows a crash notice. Release backtraces have no symbols (`strip = true`, no PDB): C3 added.
- 2026-10-19 — C1, C2 and review fixes merged to main; gate passed.
- 2026-10-29 — Amended from the 2026-10-28 research (D88): native crashes get minidumps through `crash-handler` 0.8.1 + `minidumper` 0.11.0, as Zed. Added D6, C4 (crash monitor, measured start-up cost), C5 (dumps in the notice and the report) and Q1 (dumps hold unredacted memory). C4 waits for C3, so dumps resolve against the shipped PDB.
