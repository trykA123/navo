# Experimental retention activation boundary

Status: done · Repo: `navo-beta-opus-fixes` · Branch: `backend/beta-recovery-retention` · Started: 2026-10-26

## Goal

Default mode does not acquire retention mutation leases or consult the retention catalog.
Experimental mode keeps its coordination and protection. Delete confirmations reflect active retention.

## Decisions

- D1 — Owner-approved Opus review sections 2–3 define B1–B5 and their order.
- D2 — V8 is parked on `backend/beta-v8-intersections` at `155e2ef7`; this work starts at `9d77c450`, schema V7.
- D3 — Implement directly; only the existing Sol 6.1 xhigh reviewer may review. No implementation delegation.
- D4 — Keep reserved retention-directory protection in default mode. Dropping the flag leaves containers in place.
- D5 — Before each Cargo command wait for all other Cargo/rustc processes. Use nice 10 and eight build jobs.
- D6 — No alpha changes, push, merge, or rebase.

## Components

| Component | Layer | Contract | Change | Test |
| --- | --- | --- | --- | --- |
| mutation lock | adapter | test-scoped path → lease | B5 isolated test directory | lock tests |
| profile jobs | service | mode + operation → guarded journal | B1/B2 explicit activation | red then green activation probes |
| startup and writers | service | mode + write/recovery → optional lease | B1 propagate mode | startup/save/create/Undo probes |
| retention catalog | adapter | live inventory → existence | B2 V7 fast path | empty/live/terminal catalog tests |
| delete confirmation | UI | plan + active retention → wording | B3 background state | inactive/active confirmation tests |
| retention docs | documentation | flag removal → explained behavior | B4 | review |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | mutation lock | main | high | — | isolated path | done |
| A2 | profile jobs | main | high | A1 | red B1/B2 probes, green fixes | done |
| A3 | startup and writers | main | high | A2 | mode boundary tests | done |
| A4 | retention catalog | main | high | A2 | V7 existence fast path | done |
| A5 | delete confirmation | main | high | — | red B3 probe, active state tests | done |
| A6 | retention docs | main | medium | A2–A5 | B4 + resolution report | done |
| A7 | validation | main + reviewer | xhigh | A1–A6 | affected crates, full gate once | done |

## Done when

B1–B5 have regression coverage. Experimental coordination remains enabled. The affected checks and full gate pass.
Results are appended to `docs/reviews/2026-10-25-beta-opus-resolution.md`; all changes stay local.

## Log

- 2026-10-26 — Parked V8 and returned this worktree clean to `9d77c450`. Read Opus sections 2–3.
- 2026-10-26 — Three initial B1/B2/B3 probes failed before implementation. Affected nextest: 3,621 passed, six skipped, 103.7 seconds. Fixed two GUI function-size violations without changing lint rules; affected Clippy and app build passed.
- 2026-10-26 — Sol 6.1 xhigh read-only review found no actionable issue. Four native default-mode delete confirmations passed at 1280×720 and 640×480 in both themes; screenshots inspected. Cancel preserved 100,001 files and created zero jobs each time. A broad pixel detector initially matched a marked row; narrowed it to the confirmation button and repeated all four cases successfully.
- 2026-10-26 — Full gate ran once through all ten repository steps in their configured order. Every step passed: format, workspace Clippy, Taplo, typos, size, l10n, deny, audit, nextest and doc-tests. Workspace nextest: 7,331 passed, 17 skipped, 115.7 seconds execution; gate wall time 455.3 seconds includes waiting for external builds. Every Cargo start waited for no other Cargo/rustc process; nice 10 and eight build jobs applied.
- 2026-10-26 — Final GUI wiring delta also passed Sol 6.1 xhigh review. Durable logs, four native screenshots, cancellation evidence and gate runner: the local evidence folder. Windows runtime remains unexecuted. No alpha changes, push, merge or rebase.
