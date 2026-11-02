# P5 · Diagnostics

Status: done (G5 passed 2026-10-15) · Branch: `p5-diag`

## Goal

From now on every test build records what went wrong without recording who the user is: redacted structured logs, a bounded ring of recent events, memory and performance statistics, and a report the user saves by hand.

## Decisions

- D20 and architecture "Diagnostics". Redaction happens when an event is recorded, through types.
- `tracing` with a custom layer; rolling files 3 × 10 MB in the local data folder.
- Memory statistics come through a `MemoryProbe` trait: a fake for tests, `/proc/self/status` on Linux, `GetProcessMemoryInfo` on Windows (in `navo-app`, which may use `unsafe`).
- Coverage floor `navo-diag = 85` added in L1.
- Briefs: [L1](2026-10-15/p5-l1-brief.md), [L3](2026-10-15/p5-l3-brief.md), [L7](2026-10-15/p5-l7-brief.md), [L5](2026-10-15/p5-l5-brief.md). Red atoms run on `checker` with Opus `medium` (D34).
- Redaction lives in one `Redactor` (salt, identity, UNC server table), so `<server:N>` numbering is stable within a report. Segment tokens are 4 hex characters of SipHash over salt and the lowercased name (std only).
- The memory target metric is private memory (G2 proposal): Linux `RssAnon`, Windows private bytes.

## Components

| Component | Layer | Contract | Test |
| --- | --- | --- | --- |
| `redacted_path` | lib | `RedactedPath::new(path, &Salt) -> RedactedPath` with shape fields; `Display` | proptest: no input segment longer than 2 characters appears in the output |
| `redact_text` | lib | user, computer, domain and UNC server names → tokens | unit |
| `event_ring` | lib | bounded ring by bytes; push, snapshot | unit + proptest: never over budget |
| `log_layer` | adapter | `tracing` layer → ring + rolling files, refusing raw path fields | unit |
| `memory_stats` | lib | `MemoryProbe` + cache and task counters → `MemoryReport` | unit with fakes |
| `report` | adapter | build `report.json` + summary → zip | snapshot + **canary test** |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | `redacted_path`, `redact_text` Red | T2 | checker | high | G4 | tests compile, fail |
| L2 | `redacted_path`, `redact_text` Green | T3 | api-builder | xhigh | L1 | `cargo nextest run -p navo-diag redact` |
| L3 | `event_ring` Red | T2 | checker | medium | G4 | tests compile, fail |
| L4 | `event_ring` Green | T1 | patcher | high | L3 | `… ring` |
| L5 | `log_layer` Red | T2 | checker | high | L2, L4 | tests compile, fail |
| L6 | `log_layer` Green | T3 | api-builder | xhigh | L5 | `… log_layer` |
| L7 | `memory_stats` Red | T2 | checker | medium | G4 | tests compile, fail |
| L8 | `memory_stats` Green | T3 | api-builder | xhigh | L7 | `… memory` |
| L9 | `report` + canary Red | T2 | checker | high | L6, L8 | tests compile, fail |
| L10 | `report` Green | T3 | api-builder | xhigh | L9 | `… report`, canary passes |
| L11 | wire into `navo-app` | T1 | patcher | medium | L10 | `navo --diag-report out.zip` writes a zip whose `report.json` validates |
| G5 | gate | — | orchestrator + reviewer | high | L1–L11 | gate, CI on both OS, review |

L1, L3 and L7 run in parallel.

### Briefs (outline)

- **L1/L2:** shape kept: drive letter or `<server:n>\<share:n>`, depth, extension, length class, flags for long path and non-ASCII; each segment becomes `<d:xxxx>` or `<f:xxxx>` from a salted hash; the user profile root becomes `<user>`.
- **L3/L4:** ring holds serialised events; pushing past the byte budget drops the oldest; snapshot copies without blocking pushes for long.
- **L5/L6:** a field named `path` must be a `RedactedPath`; a raw string path field is dropped and counted as a redaction miss in the next report.
- **L7/L8:** report fields: working set, private bytes, each registered cache's bytes against its budget, task counts per owner.
- **L9/L10:** canary test runs a scripted session with a known user name, computer name, server, paths and a search query, then searches the logs and the report for each; any hit fails.
- **L11:** the command-line flag stands in for the Help menu until the UI exists.

## Done when

- The canary test passes in CI on Linux and Windows.
- `navo --diag-report` writes a readable report with memory and timing sections.
- `navo-diag` coverage ≥ 85 %.

## Log

- 2026-10-15 — P5 starts on `p5-diag` in parallel with P3's gate and P4, at the owner's request to keep implementing. L1, L3 and L7 (checker, Opus medium) start in parallel.
- 2026-10-15 — L1, L3 and L7 done (checker, Opus medium): 66 redact, 25 ring and 26 memory tests. The orchestrator changed L1's text rule after the Red report: paths in free text run over spaces to a hard boundary (privacy over readability). L2 and L8 done (api-builder, gpt-6.1-sol xhigh), reviewed by hand; L4 done (orchestrator). 117 diag tests pass; redact coverage 99 %. L5 starts.
- 2026-10-15 — L11 done (orchestrator): `navo --diag-report <file.zip>`; Windows memory through `K32GetProcessMemoryInfo` and `GlobalMemoryStatusEx`. G5 took five review rounds (Opus high). Fixed, each with a failing test first: allow-listed extensions only (a dotted user name such as `john.smith` leaked `.smith`), file names end at their last allow-listed extension word, `//`, `/`, `~/`, `./`, `file://` and `smb://` paths, drive paths after words, name-carrying log fields (`file`, `folder`, `name`, `src`, `dst`, … need a `RedactedPath`), quoted fragments become `<text:N chars>`, share names replaced as whole words, apostrophes inside names, report events parsed before redaction, stable second pass, linear time on long input, and the report never replaces an existing file (`hard_link`). The extension allow-list adds `bin` and `dat` to the brief's list.
- Known limit, accepted: free-text redaction is heuristic. A bare name that is neither a path nor quoted (`renamed Budget into Plan`) cannot be recognised. Rule for code: names and paths reach logs only through path-carrying fields, which the log layer enforces by field name; messages never format a name in. Over-redaction (a Debug `by:\n` read as a path) is accepted.
