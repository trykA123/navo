# P26 · One instance and `navo <path>`

Status: done (Windows CI and the owner's check pending) · Repo: `<repo>` · Branch: `p26-iN` per atom · Started: 2026-10-19

## Goal

`navo D:\work` opens that folder, and a second launch never starts a second Navo on the same data: it hands its request to the running window, which comes to the front. Today two launches run two processes that both write the session, settings ledger and databases (architecture: "Single writer"). Roadmap Alpha item "command line"; it also unblocks "Open with Navo" and the default-file-manager option later.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Positional path.** `navo [flags] [<path>]`, at most one positional argument, anywhere among the window flags. A folder opens in a new tab in the active pane. A file opens a new tab on its folder with the cursor on the file. A path that does not exist does not stop the start: the window opens as usual and shows the status notice "“{path}” was not found". `--left`/`--right` keep their current rules (D1 of P23: refused when missing). `--version` and `--diag-report` still stand alone.
- **D2 — Who is "the same instance".** One instance per profile folder: the lock name is derived from the absolute, case-folded profile data folder (a short hash), so `--profile <dir>` and `--metrics` (temporary profile) runs are independent of the user's normal Navo.
- **D3 — Lock and channel.** Windows: a named mutex `Local\Navo-<hash>` held for the process lifetime, and a named pipe `\\.\pipe\Navo-<hash>` served by the first instance on a background thread. Other systems: an exclusive `flock` on `<runtime dir>/navo-<hash>.lock` and a Unix socket `<runtime dir>/navo-<hash>.sock` (runtime dir: `$XDG_RUNTIME_DIR`, else the temp dir); a stale socket file left by a crash is removed by the instance that holds the lock.
- **D4 — Hand-off.** A second launch with window flags connects, sends one request, waits for one reply ("ok") for up to 2 s, then exits 0. The request is one JSON line: `{"v":1,"path":…,"left":…,"right":…,"view":…}` with absolute paths (made absolute against the second launch's working folder before sending). On Windows the second launch calls `AllowSetForegroundWindow(ASFW_ANY)` before sending. If it gets no reply in 2 s it prints "navo: the running Navo did not answer" and exits 1; it never starts a second instance while the lock is held.
- **D5 — First instance.** Each request runs on the UI thread: `path` as D1; `left`/`right` navigate those panes; `view` opens the viewer as `--view` does; then the window is restored if minimised and activated. A malformed or unknown-version request gets the reply "error" and changes nothing.

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| I1 | `cli` positional path (D1 parsing); `instance` module in navo-app: lock, server thread, client, request type (D2–D4), with an injected handler; `main` holds the lock or hands off | api-builder (Codex sol xhigh), then reviewer (Opus medium) | xhigh | — | cli tests; instance tests on Linux with two instances in one test process (temp runtime dir): lock held, request delivered, reply ok, malformed request rejected, stale socket recovered, timeout path; gate | done |
| I2 | First-instance handling in the UI (D1 tab rules, D5) and the not-found notice | ui-builder (Opus medium) | medium | I1 | UI tests: a request opens a new tab on a folder, a file selects itself, missing path shows the notice, left/right navigate; gate | done |

## Done when

- On Windows: double-launching Navo brings the first window forward; `navo C:\Windows` from a terminal opens a new tab there in the running window. Checked by the owner on the next alpha.
- Instance tests pass on Linux and Windows CI; the gate passes.

## Log

- 2026-10-19 — Plan written. No instance lock exists today; `cli.rs` accepts only flags.
- 2026-10-19 — I1 by Codex sol xhigh; Opus review PASS WITH FIXES (must: start without a lock when no profile folder; ConnectNamedPipe success means listening; idle poll 200 ms; pipe per session with a current-user DACL, start without a server when it cannot be made; private Unix runtime folder; a broken hand-off reads as "did not answer"), fixed by a second Codex run. I2 by Opus: a futures channel drained by `spawn_in`, the same pattern as settings news; a missing path shows a notice. Merged together. Untested headless: real foregrounding of a minimised window on Windows.
