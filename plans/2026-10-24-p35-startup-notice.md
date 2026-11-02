# P35 startup recovery and persistent notice

Status: complete on Linux; native Windows verification pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-24

## Goal

Activate the completed P35 storage backend before normal database connections open. Recover only from the verified replacement journal and keep a recovery notice visible across clean restarts until the user dismisses it.

## Decisions

- D1 — Continue the local backend branch. The main agent implements everything; Sol 6.1 xhigh performs read-only review. No merge, push or deployment.
- D2 — Serialize startup with a separate lock before acquiring the existing database session lock. Only a sole session may check, quarantine or recover. Downgrade to a shared session while still holding the startup lock.
- D3 — Each opened Db retains the session lease until its connection closes, including background settings connections. A failed downgrade retains startup ownership until all connections close.
- D4 — Replace the global unclean flag with one preflight check and a per-profile access gate. Any failed preflight blocks every later database open for that run, even if writing the sentinel fails. Busy, I/O and future-version errors never trigger quarantine.
- D5 — Preserve a durable quarantine receipt before removing the move record. Keep it until the replacement and recovery notice are committed, so interruption after quarantine cannot lose the source slot.
- D6 — Record recovery phases before mutations. Build into a new staging database, retain interrupted attempts, sync/close the verified candidate, and promote without replacement. Persist the candidate's existing file identity so a resumed hard-link promotion can recognize its own names without touching replacements.
- D7 — Persist complete manual-recovery details before promotion. Run normal job recovery only from the promoted trusted journal, with the original attempt count persisted for retries. Commit the notice before acknowledging the receipt.
- D8 — Recovery metadata uses versioned, bounded JSON and fixed filenames derived from validated positive identifiers. Refuse malformed records, symbolic links and ambiguous promotion; preserve existing data and show a persistent failure notice.
- D9 — Extend the existing crash notice with translated recovery text, a details area and an Open folder action. Keep focus on the file list initially, support existing keyboard actions, and retain recovery notices until explicit dismissal. Saving a diagnostic report alone does not dismiss a recovery notice.
- D10 — Reuse existing tokens, buttons, file hooks, atomic writes, identity checks and dependencies. Verify native Linux screenshots in both themes at desktop/window-minimum widths. Native Windows checks remain explicit if no runner is available.

## Components

| Component | Layer | Contract (in → out) | Change | Test |
| --- | --- | --- | --- | --- |
| Quarantine receipt | store adapter | preserved slot → durable receipt and acknowledgement | extend | interruption and receipt regressions |
| Profile access lease | app/store | startup ownership → guarded database connections | new | concurrent sessions and connection lifetime |
| Recovery workflow | app service | profile, unclean flag, provider → ready or blocked store and durable notice | new | real corrupted SQLite, retries, injected job failures |
| Recovery notice | app/UI | persisted recovery summary → translated notice and folder action | extend | restart/dismiss/save/focus tests and screenshots |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | Quarantine receipt | self | high | — | failed-first receipt regressions | done (Linux) |
| S2 | Access lease and gate | self | high | S1 | database lifetime and blocked-open tests | done (Linux) |
| S3 | Workflow persistence | self | high | S1 | record validation and phase-resume tests | done (Linux) |
| S4 | Rebuild and promotion | self | high | S3 | damaged SQLite and replacement-identity tests | done (Linux) |
| S5 | Job recovery and startup wiring | self | high | S2, S4 | trusted-journal and multi-session tests | done (Linux) |
| S6 | Persistent notice service | self | high | S5 | clean restart, save and dismissal tests | done (Linux) |
| S7 | Notice UI and gallery | self | high | S6 | headless interaction tests and inspected screenshots | done (Linux) |
| S8 | Gate, docs and review | self; Sol 6.1 reviewer | xhigh | S7 | full gate and resolved review findings | done (Linux); no unresolved findings |

## Done when

- A crash and damaged database preserve the original files, recover verified state, and open a healthy journal.
- Untrusted journals and failed preflights execute no recovery from damaged state and block all normal database opens.
- Interrupted quarantine, rebuild, promotion and recovery resume safely, preserving replacement files.
- Another live database session prevents quarantine, including after the main startup guard is dropped.
- The notice identifies preserved files, resets and recovery outcomes, opens the recovery folder, and survives clean restarts until dismissed.
- Available gates, native screenshots and Sol 6.1 xhigh review pass; Windows limits are recorded separately.

## Log

- 2026-10-24 — Rechecked the existing backend and startup. Found a gap between quarantine completion and durable workflow state, and the existing journal lock was acquired after SQLite opens. This plan closes both before activating recovery.
- 2026-10-24 — Receipt regressions failed first, then all 11 Linux quarantine tests passed. Implemented preflight ownership, guarded connections, durable phases, closed/synced staging and identity-safe promotion. Added real SQLite damage, malformed metadata, session lifetime, sentinel write failure and interrupted-phase regressions.
- 2026-10-24 — Wired trusted journal recovery and translated persistent notices. Failed cleanup stays manual; later successful retries update the notice. Saving a diagnostic report keeps recovery visible; folder and keyboard actions retain focus behavior. Updated the command registry and four snapshots for Alt+O.
- 2026-10-24 — Inspected four native Linux Cage/gallery screenshots at 1280 and 1100 px in light/dark themes. All content and actions fit; existing muted/card tokens pass contrast at 6.98:1 light and 7.22:1 dark. Artifacts: the local evidence folder.
- 2026-10-24 — Sol 6.1 xhigh found two P2 notice issues: a retry committed before notice refresh could leave stale counts, and dismissal also cleared older undisplayed cases. Both regressions failed first. Refresh now reads committed terminal outcomes with a stable creation/kind/plan fingerprint, preserves missing/pruned jobs as manual, and rejects reused job identifiers. Dismissal removes only the newest three displayed snapshot cases. The reviewer confirmed both fixes and no remaining actionable findings. All 141 targeted tests passed; full gate is running. Logs: a local verification log, a local verification log, a local verification log.
- 2026-10-24 — The full gate caught four UI/gallery files over the existing 400-line limit and dynamically composed localization keys. Moved cohesive command/sample/page metadata into child modules, used static message keys and removed the unused hardcoded crash title. The localization ratchet decreased from 677 to 675. Replaced the two raw rebuild PRAGMAs with equivalent rusqlite helpers. No lint or size limits were relaxed.
- 2026-10-24 — Final `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate` passed all 10 steps in 209.0 seconds: 7,117 tests passed, 14 skipped, plus documentation tests. The eight existing allowed audit warnings are unchanged. Logs: a local verification log, a local verification report.
- 2026-10-24 — Built the final app/gallery binaries and ran Navo twice through native headless Cage using only an isolated temporary profile, empty pane folders and disabled indexing. A non-SQLite database was preserved byte-for-byte, its replacement passed a real SQLite integrity check, receipt/sentinel state cleared, and the same notice survived a clean restart. Both runs exited normally through the metrics harness. Inspected the real light/dark app screenshots in the local evidence folder; log a local verification log. No live data, merge, push or deployment. Native Windows sharing/interruption/UI verification remains open.
