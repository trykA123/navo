# Fuzzy matcher benchmark (P2 S5)

Run on the Linux dev PC (Intel Core Ultra 5 245K), release build, one thread, `cargo run --release --bin report` in `spikes/fuzzy-bench`. Versions: nucleo-matcher 0.3.1, frizbee 0.13.0.

Corpus: 2000000 paths; seed 0x52555354595f5032; FNV-1a c7e658d2fc3d0a32; 164663 Unicode paths; depths 1–8.
19 unique quality targets replace evenly spaced generated entries; folder rows use the first 100000 basenames.

Timing: median of 3 fresh-query samples after one warm-up; preparation excluded.
Scoring every name, selecting and sorting top 100; score descending, input index ascending on ties.
One thread; Rust 1.99.0 release defaults; heap tracking disabled during timing.
Nucleo: match_paths, smart case/normalization, borrowed ASCII and prepared Unicode.
Frizbee: default delimiter bonuses, smart case/Unicode, no typos; +typos allows min(chars/4, 2).
Both use literal fuzzy queries and identical partial selection; no incremental filtering or cancellation.

| Dataset | Matcher | Query | Characters | Median ms | Returned |
|---|---|---|---:|---:|---:|
| 2M paths | nucleo | `r` | 1 | 104.065 | 100 |
| 2M paths | frizbee | `r` | 1 | 73.048 | 100 |
| 2M paths | frizbee+typos | `r` | 1 | 70.323 | 100 |
| 2M paths | nucleo | `crt` | 3 | 362.956 | 100 |
| 2M paths | frizbee | `crt` | 3 | 143.186 | 100 |
| 2M paths | frizbee+typos | `crt` | 3 | 84.269 | 100 |
| 2M paths | nucleo | `cargotom` | 8 | 134.787 | 100 |
| 2M paths | frizbee | `cargotom` | 8 | 53.923 | 100 |
| 2M paths | frizbee+typos | `cargotom` | 8 | 173.141 | 100 |
| 100k folder basenames | nucleo | `r` | 1 | 2.728 | 100 |
| 100k folder basenames | frizbee | `r` | 1 | 2.074 | 100 |
| 100k folder basenames | frizbee+typos | `r` | 1 | 1.968 | 100 |
| 100k folder basenames | nucleo | `crt` | 3 | 2.515 | 100 |
| 100k folder basenames | frizbee | `crt` | 3 | 1.226 | 100 |
| 100k folder basenames | frizbee+typos | `crt` | 3 | 1.177 | 100 |
| 100k folder basenames | nucleo | `cargotom` | 8 | 2.189 | 100 |
| 100k folder basenames | frizbee | `cargotom` | 8 | 1.287 | 100 |
| 100k folder basenames | frizbee+typos | `cargotom` | 8 | 2.464 | 100 |

Memory: requested live heap, MiB; excludes allocator overhead, stack and RSS.
Each query starts a fresh matcher; peak extra excludes resident names/preparation and includes all candidate results and top-100 selection.

| Matcher | Query | Raw names MiB | Extra prepared MiB | Total haystack MiB | Peak query extra MiB |
|---|---|---:|---:|---:|---:|
| nucleo | `r` | 124.856 | 80.047 | 204.904 | 16.127 |
| nucleo | `crt` | 124.856 | 80.047 | 204.904 | 16.127 |
| nucleo | `cargotom` | 124.856 | 80.047 | 204.904 | 1.127 |
| frizbee | `r` | 124.856 | 0.000 | 124.856 | 16.005 |
| frizbee | `crt` | 124.856 | 0.000 | 124.856 | 16.009 |
| frizbee | `cargotom` | 124.856 | 0.000 | 124.856 | 1.021 |
| frizbee+typos | `r` | 124.856 | 0.000 | 124.856 | 16.005 |
| frizbee+typos | `crt` | 124.856 | 0.000 | 124.856 | 16.009 |
| frizbee+typos | `cargotom` | 124.856 | 0.000 | 124.856 | 8.021 |

Quality: intended target's 1-based rank over all 2M paths; deterministic input-order ties.

| Kind | Query | Intended target | nucleo | frizbee | frizbee+typos |
|---|---|---|---:|---:|---:|
| abbreviation | `fbr` | `/projects/Navo/docs/fuzzy_bench_report.md` | 1 | 1 | 1 |
| substitution | `fuzzy_bench_reporx` | `/projects/Navo/docs/fuzzy_bench_report.md` | not in top 100 | not in top 100 | 1 |
| substitution | `cargp.toml` | `/projects/Navo/Cargo.toml` | not in top 100 | not in top 100 | 1 |
| numbers | `rpt2026` | `D:/Documents/Reports/Annual_Report_2026.docx` | 4 | 4 | 4 |
| path fragment | `src/main` | `/projects/Navo/src/main.rs` | not in top 100 | not in top 100 | not in top 100 |
| camel case | `RI` | `/projects/Navo/src/NavoIndex.rs` | 1 | 1 | 1 |
| camel case | `FDlg` | `/projects/Navo/ui/FileDialog.rs` | not in top 100 | not in top 100 | not in top 100 |
| dashes | `fuzzy-bench/cargo` | `/projects/Navo/spikes/fuzzy-bench/Cargo.toml` | 1 | 1 | 1 |
| snake case | `appcfg` | `/projects/Atlas/config/app_config.toml` | not in top 100 | not in top 100 | not in top 100 |
| transposition | `downlaodtask` | `/projects/Atlas/src/DownloadTaskHandler.rs` | not in top 100 | not in top 100 | not in top 100 |
| path fragment | `img/hero` | `/projects/Website/img/hero-banner.png` | 1 | 1 | 1 |
| spaces | `launchplan` | `D:/Documents/Product Launch Plan.docx` | 1 | 1 | 1 |
| path fragment | `db/mig` | `/projects/Atlas/db/migrations/2026-10-01-init.sql` | 1 | 1 | 1 |
| path abbreviation | `jellycomp` | `/srv/jellyfin/docker-compose.yml` | 1 | 1 | 1 |
| snake case | `testparse` | `/projects/Navo/tests/parser_cases.rs` | 1 | not in top 100 | not in top 100 |
| extra character | `readmme` | `/projects/Navo/README.md` | not in top 100 | not in top 100 | not in top 100 |
| abbreviation | `tascfg` | `/projects/Atlas/tsconfig.json` | not in top 100 | not in top 100 | not in top 100 |
| Unicode Latin | `résumé` | `D:/Documents/résumé_2026.docx` | not in top 100 | not in top 100 | not in top 100 |
| Unicode CJK | `東京` | `/media/Photos/東京-trip-2026.png` | 1 | 1 | 1 |
| Unicode Latin | `café` | `/home/admin/Documents/café_notes.md` | not in top 100 | not in top 100 | not in top 100 |

Targets in top 100: nucleo 10/20; frizbee 9/20; frizbee+typos 11/20.

Report elapsed: 11.47 s.
