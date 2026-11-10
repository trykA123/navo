# Research picks: crates, tools, Windows speed, competitors

Status: proposed · Repo: `navo` · Started: 2026-11-10

Six researchers (2026-11-10) looked at alternatives to our crates, crates we could adopt instead of own code, Rust tooling, Windows file I/O, and competitor features. This plan keeps only picks with evidence. Every speed claim is measured with the speed-check benches before adoption; a pick that does not measure faster is dropped. Version bumps live in `2026-11-10-deps-latest.md`, not here.

## Picks

| Id | Pick | Evidence | Gate to adopt | Size | Priority |
| --- | --- | --- | --- | --- | --- |
| R1 | Natural sort for names (`file2` before `file10`) | `navo-fs/src/view/sort.rs` compares lowercase text only (checked by hand) | sort tests incl. digits, leading zeros, Unicode digits; bench on 5,000-file folder | small | high |
| R2 | Native crash reports: `crash-handler` + `minidumper` | Only a panic hook exists; access violations are not reported | a forced access violation in the tester build writes a minidump; no private data in it | medium | high |
| R3 | `cargo auditable` release exe + `cargo audit bin` in `release.yml` | cargo-auditable 0.7.7; dependency list < 4 kB in the exe | release dry run passes; audit runs on the shipped exe | small | high |
| R4 | One dependency bot (Renovate or Dependabot), weekly grouped PR, exact `=` pins | Owner rule: always on latest | PR goes through the deny gate and VM CI | small | high |
| R5 | `cargo shear` monthly; remove unused deps | Found `windows-core` declared but unused | clean run | small | medium |
| R6 | File watcher on `notify` 8.2 + `notify-debouncer-full` (replaces ~830 own lines) | mature, maintained (2026-11-08) | our watch tests pass unchanged: rename pairing, overflow → rescan, 100/500 ms debounce, network share | medium | medium |
| R7 | Index service install/start via `windows-service` (keep our ACL code) | windows-service 0.8.1 | service install/update test in the VM | small | low |
| R8 | Font viewer: `ttf-parser` → `skrifa` | ttf-parser is in maintenance mode and recommends fontations | font viewer tests; 3 call sites | small | medium |
| R9 | Fuzzy: benchmark `frizbee` against `nucleo-matcher` | self-reported 4× (1 thread) / 10× (8 threads) on 1.4 M paths; nucleo-matcher unreleased since 2024 | ≥ 2× on the real 2.6 M index and the ranking tests pass | medium | medium |
| R10 | `flate2` on the `zlib-rs` backend | zlib-rs claims; native gain unverified | faster on an archive corpus | small | low |
| R11 | Folder listing: `FindFirstFileExW` + `FindExInfoBasic` + `FIND_FIRST_EX_LARGE_FETCH` (check what we use first) | Microsoft docs | faster on local 5,000-file and SMB 50,000-file folders vs today and vs Total Commander | small | high |
| R12 | Same-server SMB copy via `FSCTL_SRV_COPYCHUNK` | data stays on the server | copy test on a real share; falls back cleanly | medium | high (2 PB shares) |
| R13 | Block cloning for same-volume copies on ReFS / Dev Drive | Microsoft engineering post | copy test on a Dev Drive in the VM | small | low |
| R14 | SMB folders: first rows paint from the first 1 MB query; re-list on demand instead of change notifications | MS-SMB2 credits; ReadDirectoryChangesExW limits on SMB | first-paint time on a 50,000-file share | medium | medium |
| R15 | Dev profile `debug = "line-tables-only"`; measure `sccache` across worktrees | Cargo docs; target dirs are 30–70 GB | target size and cold build time before/after | small | medium |

## Navo Compare (standalone) additions

- C-live — **Live working-copy compare** (Kaleidoscope 7, 2026-07): the diff and Stream update as files change. Our watcher and Stream make it cheap. Compare 2.
- C-syntax — Syntax-aware text diff with tree-sitter (WinMerge 2.16.60). Compare 3.
- C-table — Table compare with sheets (Beyond Compare 5). Compare 3, with the CSV/Excel item.
- C-patch — Open `.patch` / `.diff` files as a diff (Beyond Compare 5.2). Compare 2.

## Navo candidates from competitors (check whether we have them)

Duplicate finder, checksums (XXH128, BLAKE3) with verify, repeat last command, per-folder remembered sort and view (Double Commander request open since 2022), formula search filters (Everything 1.5). No subscription for updates is a selling point against Directory Opus.

## Rejected

panic = "abort" (we catch panics), opt-level "s"/"z" (size is not a goal), cranelift (no unwinding on Windows), BOLT (ELF only), `sonic-rs` (needs `target-cpu=native`), `mupdf` (AGPL), `hayro` for now (no text, links or outline), `gxhash` (no fallback without AES-NI), Turso (pre-1.0), `compio`/IoRing (pre-1.0, Windows 11 only), `git2` (C, no proven gain over gix).

## Log

- 2026-11-10 — Written from six researcher reports. Next: owner picks the order; R1, R3, R4 and R11 are small and high value.
