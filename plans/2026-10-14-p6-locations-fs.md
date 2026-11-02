# P6 · Locations and file systems

Status: done (G6 passed 2026-10-15) · Branch: `p6-fs` · After: G5 for merging

## Goal

Navo reads folders through providers instead of raw paths. A fake file system with fault injection and a corpus of hostile names prove the behaviour on Linux CI; the local provider lists real folders on Linux and Windows in streamed batches; I/O queues keep each disk and share within limits; one error model describes every failure.

## Decisions

- D19 (drives, network), D21 (locations, fake file system, hostile names, error model, I/O queues).
- From G3 review (2026-10-15): `Effect::ListFolder` and `Event::ListingLoaded` get a request number; the tab keeps the pending number and accepts only the matching answer, so a late answer for the same folder cannot overwrite a newer one. A refresh of the same folder (watcher) keeps the marks by matching names instead of resetting the selection. Change the event form in F2/F3, before logs from users exist.
- `Location { provider: ProviderId, path: ProviderPath }` lives in `navo-core`; `ProviderPath` keeps Windows names as WTF-16 so invalid UTF-16 survives.
- The `Provider` trait lives in `navo-fs`: `capabilities()`, `list(location, sink)`, `metadata(location)`; operations come in P8.
- Windows listing uses `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH`, `\\?\` paths, batches of 512 entries.
- I/O queue priorities: visible, prefetch, idle. Limits per volume: SSD 8, HDD 1, network 4, removable 2; drive type from the seek-penalty query on Windows, from `/sys/block` on Linux.
- Coverage floor `navo-fs = 80` added in F3.

## Components

| Component | Layer | Contract | Test |
| --- | --- | --- | --- |
| `location` | lib (core) | `Location`, `ProviderPath`, comparison by Windows rules | proptest: case-insensitive equality matches Windows ordinal rules |
| `error_model` | lib (core) | `FsError { kind, location, os_code }` → message + `RecoveryAction`s | unit + snapshot of every message |
| `provider` | contract (fs) | `Provider` trait, `Capabilities`, `EntryBatch` | compile-level; used by the next rows |
| `fake_fs` | adapter (fs) | in-memory tree; scripted faults: disk full, access denied, locked, network lost after N bytes, slow | unit |
| `hostile_names` | fixture (fs) | generator for long paths, invalid UTF-16, reserved names, trailing dots and spaces, mixed case | listing round-trip on fake and local |
| `local_provider` | adapter (fs) | streamed listing; `windows.rs`, `unix.rs` | integration on temp dirs; Windows job |
| `io_queue` | service (fs) | per-volume queues with priorities and limits; cancel by owner | unit + proptest: never above the limit |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check |
| --- | --- | --- | --- | --- | --- | --- |
| F1 | `location`, `error_model` Red | T2 | checker | high | G5 | tests compile, fail |
| F2 | `location`, `error_model` Green | T4 | orchestrator | high | F1 | `cargo nextest run -p navo-core location error` |
| F3 | `provider` contract + `fake_fs` Red | T2 | checker | high | F2 | tests compile, fail |
| F4 | `fake_fs` Green | T3 | api-builder | xhigh | F3 | `cargo nextest run -p navo-fs fake` |
| F5 | `hostile_names` | T2 | checker | high | F4 | corpus tests pass on `fake_fs` |
| F6 | `local_provider` Red | T2 | checker | high | F5 | tests compile, fail |
| F7 | `local_provider` Linux Green | T3 | api-builder | xhigh | F6 | `… local` on Linux |
| F8 | `local_provider` Windows Green | T3 | api-builder | xhigh | F7 | `… local` on Windows CI, hostile corpus included |
| F9 | `io_queue` Red | T2 | checker | high | F4 | tests compile, fail |
| F10 | `io_queue` Green | T4 | orchestrator | high | F9 | `… io_queue` |
| G6 | gate | — | orchestrator + reviewer | high | F1–F10 | gate, CI on both OS, review |

F9–F10 run in parallel with F5–F8.

### Briefs (outline)

- **F1/F2:** error kinds: not found, access denied, sharing violation, locked, disk full, path too long, name invalid, network lost, timeout, cancelled, unknown(os code). Each maps to a user message and its recovery actions.
- **F3/F4:** faults are scripted per path and per operation count, so tests are deterministic.
- **F5:** the corpus is a function returning names; the test lists a folder containing all of them and gets every one back byte-for-byte.
- **F6–F8:** listing streams `EntryBatch`es through a bounded channel; cancelling stops within one batch; a 100k-entry folder delivers its first batch in under 20 ms on the dev PC.
- **F9/F10:** jobs carry an owner id; dropping the owner cancels its queued jobs; priority inversion is impossible because visible work preempts queued prefetch.

## Done when

- The hostile-name corpus round-trips on the fake and the local provider, on Linux and Windows.
- Faults produce the right `FsError` and recovery actions.
- `navo-fs` coverage ≥ 80 %.

## Log

- 2026-10-15 — F1 done (checker, Opus medium): 78 location and error tests. F2 done (orchestrator): `Location` = provider + lossless `ProviderPath` (text, UTF-16 with lone surrogates, or raw bytes; plane-16 escapes in the text form), NTFS-like `windows_eq`, `FsError` with Windows and errno tables, recovery actions and messages that never hold the OS text. G3 note done: listing request numbers, `pane.refresh` keeping marks by name, `list_unloaded()` for start-up. F3 done (checker): 63 provider and fake tests; `FakeFs::fault` keeps five parameters (reviewed `expect`). F4 runs on a Claude agent (Opus medium, D35).

- 2026-10-15 — F4 done (Claude agent, Opus medium): `FakeFs` with faults, 885 workspace tests. D31 cleanup: `Location::provider_root` replaces the last `expect`. `main` merged in. F5 (hostile names) and F9 (`io_queue` Red) start in parallel on `checker` (Opus medium); briefs in `plans/2026-10-15/`. F9 splits the queue into a pure `Scheduler` and a thin `IoQueue` runner with lazy per-volume threads; drive-type detection stays outside F9–F10 (`set_volume` is the input).
- 2026-10-15 — F5 done (checker): 52 hostile names (52 total, 47 Unix, 38 Windows). The corpus found a core defect: a name holding a plane-16 escape character read back as another path, so `FakeFs::metadata` could return another file. Fixed in core: `to_text` quotes literal escape-range characters with U+100800. `Location::join` added (orchestrator). F9 done (checker, 61 tests); F10 done (orchestrator): pure `Scheduler` plus a runner with lazy per-volume workers, panics caught. F6 done (checker): 49 Linux and 9 Windows tests. Decision: the local provider uses `std::fs` on both OSes (no `unsafe`; lossless names on Windows through `OsStr` WTF-8); `FindFirstFileExW` only if the 100k benchmark misses 20 ms. Errno 20 and Windows 267 map to `NameInvalid`. F7+F8 run as one atom on a Claude agent (Opus medium).
- 2026-10-15 — F7+F8 done (Claude agent, Opus medium): `LocalFs` on `std::fs`; first batch of a 100k folder in about 0.3 ms. Windows CI found one test bug (a plain path drops trailing dots). G6 review: PASS, six minors, all fixed before the merge: NTFS simple upcase for iota-subscript Greek, local Locations that would read back as a provider are refused, fake and local agree on cancel during the last batch, entries whose `stat` fails stay listed, `.`/`..` are resolved before `\\?\` (tested on Linux through `local/verbatim.rs`), `IoQueue::submit` takes a `JobSlot`. Four more `io_queue` guarantee tests. P6 merges to `main`.
