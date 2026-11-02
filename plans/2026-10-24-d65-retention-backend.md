# D65 retention backend

Status: backend complete on Linux; normal job/UI activation and native Windows pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-24

## Goal

Provide a persistent backend that preserves a file, link or folder tree on its current volume, restores it without replacing newer files, and safely purges selected retained data. This stage exposes and exercises the complete engine; normal job activation, settings, Recently deleted and general user undo follow separately.

## Decisions

- D1 — Continue from `9db52dab` in the existing isolated worktree. The main agent implements everything; the existing Sol 6.1 xhigh agent reviews only. No merge, push or deployment.
- D2 — Use the existing core, filesystem provider and profile SQLite database. Add a `navo-retention` service crate to compose those layers without making filesystem adapters depend on storage. Add no new external package.
- D3 — Policy accepts explicit 1–7 day retention, a logical-byte cap per volume and a minimum observed free-space threshold. Choose expired entries first, then oldest entries over quota. Low space selects one extra entry at a time and requires a fresh space observation; never infer physical free space from logical file sizes.
- D4 — Store retention entries in schema V6 with an optional job foreign key. Live retained entries protect their owning jobs from pruning; running, restore-pending or missing owners pin entries against restore/purge. Preserve catalog records across clean restarts.
- D5 — Preserve trees with a native atomic no-replace rename into an exclusively created sibling `.navo-retained-*` container. Require explicit provider support and stable volume/file and mount identities. Linux uses renameat2 NOREPLACE with no fallback; Windows uses non-replacing MoveFileEx. No copy or permanent-delete fallback. Never traverse links or nested mount points, including same-device bind mounts.
- D6 — Snapshot bounded relative node paths, identities, sizes and modification stamps before capture. Validate the complete tree before purge; remove only recorded matching nodes in reverse order. Unexpected or changed data stops mutation and remains available for manual inspection.
- D7 — Persist Preparing, Ready, Restoring and Purging phases before filesystem changes, then terminal states. Record parent/container identities and restore targets. Reconcile interrupted operations by identity; ambiguous or replaced files remain untouched. An interrupted preparation does not initiate a new source move.
- D8 — Serialize one engine per profile with a read/write lock file. Callers supply an already opened Db after application preflight; retain its access lease. All engine I/O belongs on a worker when activated in the app.
- D9 — Use validated versioned bounded records and paginated catalog reads. P35 salvage validates retention records independently; rejected catalogs preserve readable original/held paths and raw records for manual recovery. An untrusted job group cannot authorize retention purging.
- D10 — Exercise only temporary databases and temporary/FakeFs trees. Run targeted regressions, the full Linux gate and read-only review. Native Windows sharing/runtime verification remains explicit.

## Components

| Component | Layer | Contract (in → out) | Change | Test |
| --- | --- | --- | --- | --- |
| Retention records and policy | core | snapshots, policy, observed space → validated records and purge choices | new | bounds, expiry, quotas, pins, volume separation |
| Retention catalog | store | Db, records and transitions → durable paginated entries | new | migration, restart, malformed records, state guards and job pruning |
| Retention engine | service | catalog, provider and request → preserved/restored/purged entry | new | FakeFs faults, interrupted phases, replacement safety and actual local trees |
| Recovery compatibility | store/app | damaged catalog → verified salvage or manual detail records | extend | legacy schema, invalid payload and journal/cross-table failure |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| R1 | Shared identities and record validation | self | high | — | public record boundary regressions | done (Linux) |
| R2 | Pure retention policy | self | high | R1 | expiry/quota/pin/space regressions | done |
| R3 | Catalog migration and CRUD | self | high | R1 | V5 upgrade and real SQLite restart | done (Linux) |
| R4 | Catalog transitions and ownership | self | high | R3 | invalid transitions, pagination and pruning | done (Linux) |
| R5 | Provider snapshots and container ownership | self | high | R1 | links, identities, bounds and replacement cases | done (Linux) |
| R6 | Capture and preparation reconciliation | self | high | R3, R5 | interrupted intent/rename and exact local bytes | done (Linux) |
| R7 | Restore and restore reconciliation | self | high | R6 | collisions, target replacement and restart | done (Linux) |
| R8 | Purge and purge reconciliation | self | high | R2, R6 | pinned/modified trees, partial failures and actual local deletion | done (Linux) |
| R9 | P35 catalog salvage | self | high | R4 | malformed catalog and untrusted owner regressions | done (Linux) |
| R10 | Gate, docs and review | self; Sol 6.1 reviewer | xhigh | R7, R8, R9 | full gate and resolved read-only review | done (Linux); no unresolved findings |

## Done when

- The public engine preserves and restores real temporary files, links and nested folders across reopening the catalog.
- Capture uses same-volume rename, preserves byte content and refuses unsupported identity/rename providers before moving source data.
- Restore never overwrites an existing target; purge preserves changed files, unrecorded children and replacement containers.
- Interrupted phases reconcile without guessing ownership or treating an unavailable volume as deletion.
- Policy honors explicit retention bounds, per-volume quotas, pinned jobs and actual free-space observations.
- Catalog migration, P35 recovery compatibility, the full available gate and Sol 6.1 xhigh review pass.
- Normal operation activation and the user-facing restore/undo flow remain recorded as the next D65 stage, with native Windows verification outstanding if unavailable here.

## Log

- 2026-10-24 — Found that completed jobs immediately release A8 held files. D65 therefore needs durable retention ownership before normal delete/overwrite activation. Chose a separate service composing existing provider/storage contracts, with resumable filesystem phases and explicit policy inputs.
- 2026-10-24 — Implemented validated records/policy, schema V6 catalog and job pruning guards, public retention engine, interruption handling and P35 manual recovery compatibility. The first 61 targeted tests passed. Three additional regressions failed first: duplicate policy identifiers, source dot-segments and a restored file edited before state commit. Fixed all three; evidence a local verification log.
- 2026-10-24 — Initial reuse of generic rename capabilities exposed the local listing provider's older capability declarations. The final design keeps those declarations and the existing general rename unchanged, and adds explicit atomic-rename/mount-identity contracts for retention. Providers without them refuse retention; no fallback moves or deletes data.
- 2026-10-24 — The reviewer initially hit a usage limit. Retried after the stated reset using the same Sol 6.1 xhigh reviewer, without any implementation delegation. Review found three P1s: a replaced ancestor could redirect purge, the generic rename fallback could replace a racing target, and device identity alone did not exclude bind mounts. Native ancestor-link and early mount-list regressions failed first (a local verification log). Added ancestor guards at traversal/deletion boundaries, separate per-operation mount identity, and native atomic no-replace rename. Linux uses the existing rustix statx adapter; source: https://man7.org/linux/man-pages/man2/statx.2.html.
- 2026-10-24 — Added interrupted nested-directory purge coverage; its regression failed first, then missing already-purged ancestors were handled without traversing replacements (a local verification log). Updated V4/V5 migration fixtures to preserve their original row/schema assertions under V6. All 258 integration tests and workspace all-targets Clippy passed.
- 2026-10-24 — Follow-up review found one P2: a payload replaced by the target progress callback moved before postcondition checking. A native regression failed first (a local verification log). Moved target progress before full snapshot verification and checked the captured root immediately before atomic rename. All 27 engine tests passed. Sol 6.1 xhigh confirmed all three P1s and the P2 resolved, with no new actionable findings. Restarted the full gate on the final implementation; native Windows remains unverified.
- 2026-10-24 — Strengthened the existing app test to require readable original/container paths in the full JSONL report, beyond the capped UI hints. It failed first (a local verification log); added decoded `manual_retained` paths alongside raw records. All 26 app store-recovery tests passed. The reviewer confirmed this report-only correction has no actionable findings.
- 2026-10-24 — Final `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate` passed all 10 steps in 147.4 seconds: 7,170 tests passed, 14 skipped, plus documentation tests. No lint or size limits were relaxed and no new external dependency was added. Final logs: a local verification log, a local verification log, a local verification log.
- 2026-10-24 — Built the final app and exercised a damaged V6 database with a real retained payload and rejected owner journal through native headless Cage. Both light/dark runs exited normally, preserved exact database/payload bytes, kept the healthy catalog empty, and persisted readable original/container paths in the notice and full JSONL report across a clean restart. Inspected both 1280×720 screenshots in the local evidence folder. Log: a local verification log; JUnit: a local verification report. Only temporary data was used. Main and the original backend worktree remain unchanged; no merge, push or deployment.
- 2026-10-24 — Next D65 stage: activate capture in supported delete/overwrite jobs with journal rollback ordering, define settings/defaults, perform maintenance with fresh free-space observations and prune acknowledged terminal metadata, then expose Recently deleted and general user undo. Native Windows sharing, reparse-point and runtime validation remains open.
