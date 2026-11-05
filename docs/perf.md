# Performance

Every push to `main` runs the `perf-windows` CI job: one first run (the closest a runner gets to a cold start) and five warm runs of `navo --metrics <file.json>` on a generated fixture (one folder with 5,000 files, one with 200 folders of 25 files each). The job summary shows min, median and max next to the [v1.0 targets](roadmap.md#v10-targets).

The GitHub runner renders in software (Microsoft Basic Render Driver), so frame times are relative, not absolute. The job now fails the build, and only when the median warm first frame is above 300 ms or private bytes in any run are above 160,000,000 (2× each target). The owner's Windows run with `--metrics` is the real reference and goes here when available.

## Baseline

CI run 39154378891, commit 8654608, 18 October 2026, exe 89.2 MB.

| Measure | First run | Warm min | Warm median | Warm max | Target |
| --- | --- | --- | --- | --- | --- |
| Start to first frame | 212.5 ms | 177.3 ms | 179.8 ms | 185.2 ms | warm ≤ 150 ms |
| Start to first usable frame (both panes listed) | 261.9 ms | 224.5 ms | 236.7 ms | 244.3 ms | cold ≤ 400 ms |
| Private bytes, idle after 5 s | 67.3 MB | 66.4 MB | 67.3 MB | 67.7 MB | < 80 MB |
| Working set, idle after 5 s | 97.1 MB | 93.4 MB | 96.8 MB | 97.0 MB | — |
| Peak working set | 101.9 MB | 97.9 MB | 101.3 MB | 101.6 MB | — |

Warm start-up marks (median of five runs): main 16.0 ms, databases opened 33.1 ms, window opening 36.4 ms, app running 98.4 ms, window built 154.2 ms, first frame 179.8 ms. About 60 ms goes to starting GPUI and about 55 ms to building the window.

## Owner's PC (reference)

Work laptop, Intel UHD Graphics, build 5844556 (alpha-2026-10-19a), 19 October 2026. Run 1 is the first launch of the day; runs 2–4 are warm; run 4 used `--left C:\Windows --right C:\Users`.

| Phase | Run 1 | Run 2 | Run 3 | Run 4 | CI warm median |
| --- | --- | --- | --- | --- | --- |
| Process start to `main` | 739 ms | 292 ms | 282 ms | 76 ms | 16 ms |
| `main` to window opening | 42 ms | 46 ms | 47 ms | 53 ms | 20 ms |
| Window opening to app running (GPUI start) | 248 ms | 246 ms | 215 ms | 212 ms | 62 ms |
| App running to window built | 90 ms | 97 ms | 70 ms | 75 ms | 56 ms |
| Window built to first usable | 38 ms | 37 ms | 33 ms | 29 ms | 82 ms |
| **Start to first usable** | **1157 ms** | **716 ms** | **645 ms** | **445 ms** | 237 ms |
| Private bytes, idle | 139.1 MB | 141.8 MB | 140.8 MB | 137.6 MB | 67.3 MB |
| Working set, idle | 125.1 MB | 127.9 MB | 126.9 MB | 124.6 MB | 96.8 MB |

Against the v1.0 targets (cold ≤ 400 ms, warm ≤ 150 ms, RAM < 80 MB) all three miss on real hardware. Most of the gap is before `main` (76–739 ms, versus 16 ms on CI) and in GPUI's start on a real GPU (≈ 215–250 ms, versus 62 ms with the software renderer). Private bytes double with a real GPU.

Second set, same build, after moving `navo.exe` out of Downloads (runs 1–4; run 1 is the first launch from the new folder):

| Phase | Run 1 | Run 2 | Run 3 | Run 4 |
| --- | --- | --- | --- | --- |
| Process start to `main` | 417 ms | 103 ms | 96 ms | 88 ms |
| Window opening to app running (GPUI start) | 299 ms | 304 ms | 300 ms | 277 ms |
| **Start to first usable** | **894 ms** | **592 ms** | **590 ms** | **533 ms** |
| Private bytes, idle | 140.0 MB | 140.1 MB | 140.7 MB | 137.3 MB |

Outside Downloads the warm time before `main` drops from ≈ 285 ms to ≈ 95 ms; the first launch from a new folder is still slow (the antivirus scans the exe again). GPUI's start on this GPU stays the largest part we control (≈ 280–300 ms).

Third set, after `Unblock-File` (Mark of the Web removed): before `main` 320 / 96 / 100 / 95 ms, GPUI start 271 / 249 / 264 / 269 ms, first usable 748 / 500 / 525 / 522 ms, private bytes ≈ 140 MB. Unblocking does not change the time before `main` (≈ 95 ms warm); the lower totals come from GPUI's start, which varies by ±30 ms between sets.

Fourth set, build 3b948e3 (alpha-2026-10-21b), 21 October 2026, outside Downloads and unblocked. Run 1 is the first launch after a restart; runs 2–4 are warm; run 4 used `--left C:\Windows --right C:\Users`.

| Phase | Run 1 | Run 2 | Run 3 | Run 4 |
| --- | --- | --- | --- | --- |
| Process start to `main` | 1281 ms | 102 ms | 114 ms | 114 ms |
| `main` to window opening | 57 ms | 47 ms | 47 ms | 62 ms |
| Window opening to app running (GPUI start) | 285 ms | 292 ms | 284 ms | 257 ms |
| App running to window built | 87 ms | 83 ms | 98 ms | 77 ms |
| Window built to first usable | 35 ms | 39 ms | 48 ms | 30 ms |
| **Start to first usable** | **1745 ms** | **563 ms** | **591 ms** | **540 ms** |
| Private bytes, idle | 137.4 MB | 137.1 MB | 136.8 MB | 136.2 MB |

Warm start is where it was on 19 October (median 563 ms against 522–590 ms), although the app gained the Rich memory view, Ctrl+K sources and multi-rename. Memory is slightly lower. The cold start spends 1.28 s before `main`: Windows reads and scans the 98 MB exe after a restart. Navo's own part (from `main` to first usable) takes 464 ms cold and about 450 ms warm, so a smaller exe is the main lever for the cold start; GPUI's start (≈ 260–290 ms) is the largest part inside the app.

Same build with `NAVO_GPU=high-performance` (NVIDIA T1200 Laptop GPU), three warm runs: GPUI's start 1253 / 1238 / 1049 ms (Intel ≈ 260–290 ms), first usable 2218 / 2186 / 1825 ms, private bytes 254 / 256 / 237 MB. The discrete GPU is about three to four times slower to start and uses nearly twice the memory, so the low-power default stays.

## Heavy benchmark

`navo bench` stresses search, indexing and big folders without opening a window. Animation smoothness is measured separately with Record performance.

How to run it on a Windows PC:

1. Open a terminal in the folder with `navo.exe`.
2. Run `navo.exe bench --root D:\navo-bench --files 3000000`.
3. Wait. The terminal shows each phase, then a summary table.
4. Send `navo-bench-<date>-<time>.json` and `.md` from the folder with `navo.exe`.

Options: `--root` is a new or empty folder (the bench refuses a folder with files in it). `--files` defaults to 2,000,000. `--keep` keeps the generated files; without it the bench deletes them at the end. `--out <file.json>` writes the reports elsewhere. If the run is stopped with Ctrl+C, delete the `--root` folder by hand. The exit code is non-zero only on errors, never on slow numbers.

What each number means:

- **Fixture.** The same seeded tree every run: folders up to 12 levels deep, mixed extensions (.c .h .elf .map .a2l .dbc .pdf .png …), long and Unicode names, one folder with 500,000 files and one with 100,000. Files are empty. Generation time measures the disk, not Navo.
- **Index.** The Ctrl+K disk index built by the folder walker, the same path the app uses without the index service. The NTFS MFT reader needs the elevated service, so the bench does not measure it. Wall time, entries per second, memory before, at peak and after (private and resident), and the index file size and save time.
- **Ctrl+K search.** Eleven fixed queries through the real Ctrl+K file search (query parsing, index search, ranking, at most 500 results). Cold is the first run; p50, p95, p99 and max come from 50 warm runs.
- **Folder listing.** The two huge folders opened through the real file list: time to list, each sort (size, date, extension, name) and each keystroke of the type-to-filter text `sensor`.
- **Incremental update.** 10,000 changes (4,000 new files, 3,000 renames, 3,000 deletes) in one folder, and how long until the index shows all of them. Windows only; the bench stops waiting after 300 s and says so.

Linux result, 26 October 2026, release build of 6873ea0 with the bench added, 14 CPUs, ext4 on NVMe, 2,000,000 files in 24,423 folders (generated in 5.8 s):

| Measure | Value |
| --- | --- |
| Index: 2,024,424 entries | 2.7 s (742,610 entries/s) |
| Index memory: before, peak, after (private) | 8.2 MB, 72.7 MB, 72.8 MB |
| Process peak working set | 140.7 MB |
| Index size on disk, save time | 56.3 MB, 165 ms |
| Ctrl+K exact name, warm p50 / p99 | 9.9 / 14.1 ms |
| Ctrl+K prefix `brake_contr`, p50 / p99 | 28.7 / 40.0 ms |
| Ctrl+K one letter `a`, p50 / p99 | 33.8 / 51.8 ms |
| Ctrl+K path fragment `huge-100k/sensor`, p50 / p99 | 122.7 / 145.4 ms |
| Ctrl+K other queries (fuzzy, extension, Unicode, no match), p50 | 9.9–22.9 ms |
| 500,000-file folder: list | 933 ms |
| 500,000-file folder: sort by size, date, extension, name | 121, 297, 603, 510 ms |
| 500,000-file folder: each filter keystroke | 92–150 ms |
| 100,000-file folder: list, slowest sort, slowest keystroke | 159 ms, 93 ms, 17 ms |
| Incremental update | not measured on Linux (the folder watcher there does not cover subfolders) |

The path fragment is the slowest query, because it builds the full path of every candidate. Sorting and filtering the 500,000-file folder take a visible 0.1–0.6 s per step.

## Walk and index timing

`cargo xtask bench-walk` times folder listing, folder walking and the Ctrl+K index pieces without opening a window. It is informational only: it never fails a build and never blocks a merge.

Run it from the repository root:

```
cargo xtask bench-walk <dir> [--targets list,size] [--runs N] [--warmup N] [--prepare "<command>"]
```

- `<dir>` is required and comes first. The command creates its test folders inside it and reuses them on later runs.
- `--targets` is a comma-separated list. The default is `list,size`. `index` is short for `walk,search,save,load`.
- `--runs` is the number of recorded runs (default 10, at least 1).
- `--warmup` is the number of unrecorded runs before the recorded ones (default 2, may be 0).
- `--prepare` is a shell command that runs before each cold run, for example a cache flush.

What each target measures:

| Target | What is timed |
| --- | --- |
| `list` | Listing one folder with 5,000 files through the file provider. |
| `size` | Computing the size of a generated tree of 100,000 files in 1,000 folders. |
| `walk` | Walking that same tree with Navo's own folder walker. |
| `walk-dua` | Walking that tree with the `dua` walker, for comparison. |
| `walk-ignore` | Walking that tree with the `ignore` walker, for comparison. |
| `search` | Five fixed Ctrl+K queries over an in-memory index of 1,000,000 names. |
| `save` | Writing that index to a file. |
| `load` | Reading that index back from a file. |

How to read the table: each target gets one table with a cold row and a warm row. Times are in milliseconds. The columns are:

- **Mean ± σ:** the average and its spread. A large σ means unstable runs.
- **Median:** the middle run. Prefer it to the mean when outliers exist.
- **Min** and **Max:** the fastest and slowest run.
- **Outliers:** runs far from the median (modified Z-score above 3.5 on the median absolute deviation). One slow run from a busy disk shows up here.
- **Files/s:** files per second at the median, for the walk targets only.

Warm and cold are not the same thing. A warm run reads folders the operating system already cached. Without `--prepare`, the cold row is a single run on a freshly generated test folder, and the table says "cache not flushed". That run is only roughly cold. With `--prepare`, the command runs before each cold run and the cold row has one sample per run. Navo does not flush caches itself. A cold row is only as cold as your command makes it, and the command needs rights to do so. If the folder already exists, the cold row is empty and says "fixture reused".

On CI the Windows test job runs `bench-walk` with `--runs 3` and adds the table to the job summary. Treat those numbers as a trend on shared hardware, not as a reference.

## Profiling with Tracy

Build with `cargo build --release -p navo-app --features tracy`. The build marks every frame and sends spans at `debug` level to Tracy. Open it with the Tracy viewer that matches `tracy-client-sys` 0.30.1 (Tracy v0.14.1 per the version table in that crate's README). For sampling without a viewer, run `samply record navo.exe`.

## Shared listing (2026-11-05)

Measured by tests on Linux (D110):

| Case | Before | After |
| --- | --- | --- |
| Rows copied by one sort of a 100k-row folder | 7,100,000 B | 0 B |
| Sorts for two tabs on one folder, same key | 2 | 1 |
| Bytes allocated by one watcher patch of a 100k-row folder | 33,005,292 B | 7,000,617 B (the row clone itself is 7,000,000 B) |

Windows VM, `navo bench --files 100000`, 3 runs each, median ms:

| Case | Before | After |
| --- | --- | --- |
| List a 5k folder | 24.2 | 9.7 |
| Sort a 5k folder by size / date / extension / name | 5.95 / 4.05 / 2.79 / 1.50 | 0.77 / 0.92 / 1.04 / 0.61 |
| List a 25k folder | 23.4 | 22.8 |
| Index time, search p95, peak memory | — | unchanged within spread |

## History

| Date | Commit | Median warm first frame | Max private bytes |
| --- | --- | --- | --- |
| 2026-10-18 | 8654608 | 179.8 ms | 67.7 MB |
| 2026-10-19 | 5844556 (owner's PC, warm median) | 645 ms first usable | 141.8 MB |
| 2026-10-19 | 5844556 (owner's PC, outside Downloads, warm median) | 590 ms first usable | 140.7 MB |
| 2026-10-21 | 3b948e3 (owner's PC, warm median) | 563 ms first usable | 137.4 MB |
