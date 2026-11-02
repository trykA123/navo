# P2 · GPUI spike

Status: done (G2 passed 2026-10-15: GO, D2 validated) · Branch: `p2-spike` (merged) · After: G1 (done 2026-10-14)

## Goal

Find out with numbers whether GPUI with `gpui-kit` can carry Navo: design B's look, 100,000 rows, three panes and the drawer, inside the v1 targets. The result is a go or no-go for D2, decided with the owner at G2.

## Decisions

- Spike code lives in `spikes/` and is not part of the product. It is deleted or archived after G2; lessons go to `docs/spike/results.md`.
- `gpui-kit` at its latest release (0.7.3 today), pinned exactly.
- Visual reference: `design/prototype/dir-b.css`, `design/prototype/shared.css`, `design/screens/b4-1440.png`.
- Start from variant 1 of `spikes/gpui-b-variants` (branch `spike/gpui-b-variants`, D27): its tokens, static fonts and shell already exist. S2 and S3 extend it; S1 adds the open gaps listed in D27 (scrollbar, gpui-kit TabBar, Table and Dock not yet tried, Windows untested).
- Targets from `docs/roadmap.md`: cold ≤ 400 ms, warm ≤ 150 ms, idle RAM < 80 MB with two panes, `.exe` < 30 MB; scrolling 100k rows without dropped frames at 60 Hz.
- Measurements on three machines: Linux dev PC, `windows-latest` CI (software rendering, for start-up and memory only), the owner's Windows PC (manual run).

## Components

| Component | Layer | Contract | New | Test |
| --- | --- | --- | --- | --- |
| `gpui-kit-notes` | research | questions → `docs/spike/gpui-kit-notes.md` | new | sections present |
| `spike-shell` | ui | design B window: top bar, rail, three panes, drawer, F-key bar | new | screenshot against `b4-1440.png` |
| `spike-list` | ui | 100k synthetic rows in a virtual list, keyboard navigation, selection | new | headless test: scroll to row 99,999, select it |
| `spike-metrics` | tooling | run → JSON: first-frame time, frame times, working set, binary size | new | JSON schema test |
| `fuzzy-bench` | tooling | 2M synthetic names → table: `nucleo` vs `frizbee` | new | bench runs, prints both |
| `a11y-probe` | ui | AccessKit tree of the list → roles and names | new | headless assertion |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | `gpui-kit-notes` | T1 | researcher | medium | G1 | notes answer every question below, with links | done |
| S2 | `spike-shell` | T4 | orchestrator | high | S1 | window opens on Linux; screenshot reviewed by orchestrator | done |
| S3 | `spike-list` | T4 | orchestrator | high | S2 | headless test passes; manual scroll smooth | done |
| S4 | `spike-metrics` | T2 | ui-builder | medium | S3 | `spike --metrics` prints valid JSON | done |
| S5 | `fuzzy-bench` | T3 | api-builder | xhigh | G1 | `cargo bench -p fuzzy-bench` prints the table | done |
| S6 | `a11y-probe` | T2 | ui-builder | high | S3 | headless test finds list rows with names | done |
| S7 | Windows run | — | orchestrator | — | S4, S6 | CI job uploads metrics JSON; owner runs on Windows PC | done |
| G2 | go or no-go | — | orchestrator + owner | — | S1–S7 | `docs/spike/results.md` written; D2 updated | done |

S5 runs in parallel with S1–S4.

### Briefs (outline)

- **S1:** answer, with links to `gpui-kit` docs and source: how to depend on `gpui-kit` alone; the virtual list and table APIs and their row-height rules; the dock layout and its serialisation; the theme format (`.theme-schema.json`) and how to map our tokens; headless UI test setup; AccessKit support on Windows; Windows build needs (MSVC, DirectX); known issues on Windows in the last three releases.
- **S2:** a `spikes/gpui-b` binary reproducing design B's shell from the reference files: top bar with search, rail (pins, workspaces, drives), three panes in a horizontal split with tabs and path bars, drawer with two views, F-key bar. Tokens from `dir-b.css` as one Rust theme struct. Light and dark.
- **S3:** each pane shows 100,000 synthetic rows (name, size, date, kind) in the `gpui-kit` virtual list; Up/Down/PageUp/PageDown/Home/End, Space and Insert select, Tab switches panes. Headless test drives the keys.
- **S4:** `--metrics` flag: time from process start to first presented frame, frame-time percentiles while auto-scrolling 10 s, working set after 5 s idle, binary size; JSON to stdout. Cold and warm runs from a script.
- **S5:** `spikes/fuzzy-bench` with `divan`: 2,000,000 path-like names generated with a fixed seed; queries of 1, 3 and 8 characters; time per query and memory for `nucleo` and `frizbee`, plus result quality on 20 hand-picked queries.
- **S6:** read the AccessKit tree in a headless test: the list has the list role, rows have names, the focused row is reported.
- **S7:** a manual CI job (`workflow_dispatch`) runs the spike on `windows-latest` and uploads the JSON; the owner runs the same `.exe` on a Windows PC and pastes the JSON.
- **G2:** the orchestrator writes `docs/spike/results.md` with all numbers against the targets; the owner decides go or no-go. No-go reopens D2 with the fallback recorded in `docs/discussion.md`.

## Done when

- Every target has a measured number on Linux and a Windows PC.
- D2 says "validated" or is reopened, with the numbers linked.

## Log

- 2026-10-14 — Early Windows numbers from the variants prototype on a Windows PC (Intel UHD + NVIDIA T1200 laptop, CrowdStrike present). Release `.exe` 17.6 MB with fat LTO and `panic=unwind`.
  - Default (GPUI takes DXGI adapter 0 = NVIDIA): first frame 566–667 ms. Phases: platform and DirectWrite init ~170 ms, D3D11 device on the NVIDIA GPU ~250–310 ms, our fonts and Segoe lookup ~4 ms, window and first render ~145 ms.
  - The owner set the app's GPU to Intel UHD by hand: first frame **376 ms**; device creation drops to ~70 ms.
  - Linux dev PC for comparison: ~110 ms.
  - Actions for S2–S4: patch GPUI's `get_adapter` (gpui-pre-windows 0.3.9, `directx_devices.rs`) to prefer `EnumAdapterByGpuPreference(DXGI_GPU_PREFERENCE_MINIMUM_POWER)` unless the window's monitor is driven by another adapter, and offer it upstream; break down the ~170 ms before the run callback and the ~130 ms to the first frame; test start from the tray (hot) on Windows. The warm target (≤ 150 ms) is not met on Windows yet; revisit it with the owner after these fixes.

- 2026-10-14 — G1 passed; P2 starts. The spike branch `spike/gpui-b-variants` (`52f997a`, worktree `.crew/navo-gpui-b-variants`) holds variant 1 in `spikes/gpui-b-variants/` (2,417 lines, own workspace, `gpui-kit = "=0.7.3"`). S1 (researcher, Haiku, read-only, medium) and S5 (api-builder, gpt-6.1-sol, xhigh) start in parallel. S2 and S3 are the orchestrator's (D28).
- 2026-10-14 — S1 on Haiku used ~114k tokens (above the 90k ceiling) and twice ended without a report. Escalated one tier: S1 retry on Sonnet (general-purpose, read-only, medium) with a narrower reading plan; it writes the notes to the scratchpad for the orchestrator to commit.
- 2026-10-14 — S1 done (Sonnet retry): `docs/spike/gpui-kit-notes.md`. Spot-checked: `get_adapter` at `gpui-pre-windows-0.3.9/src/directx_devices.rs:106` walks `EnumAdapters` (no GPU preference), `Window::debug_a11y_tree_json` exists, gpui-kit ships headless tests (`tests/dock.rs`, `tests/collections.rs`). Key facts for S2–S6: `uniform_list` + `scroll_to_item` for fixed rows, `DockArea::dump/load` for layout, JSON themes via `ThemeRegistry`, rows need an explicit `ListItem` role, `#[gpui_kit::test]` with `test-support`. S2 starts (orchestrator).
- 2026-10-14 — S2 done (orchestrator, Opus): `spikes/gpui-b` is variant 1 alone (variants 2 and 3, the picker, Segoe lookup and Google Sans Flex removed), branded Navo, with a drawer that switches Transfers, Clipboard and Search (click a tab or F9). Clippy and fmt clean in its own workspace; 2,125 lines. Screenshots at 1440×900 in both themes (`spikes/gpui-b/screens/s2-*.png`) match `b4-1440.png`. Screens are taken on a headless Hyprland output (`NAVO-SHOT`, workspace 4) so nothing opens on the owner's screen; two early attempts briefly opened the window on the owner's workspace before the rule was set.
- 2026-10-14 — S5 done (api-builder, gpt-6.1-sol, xhigh): `spikes/fuzzy-bench` (own workspace, nucleo-matcher 0.3.1, frizbee 0.13.0, divan 0.1.21, licences ok). Re-run by the orchestrator: frizbee 2–4× faster than nucleo on 2M paths (e.g. `crt` 78–91 ms vs 352–360 ms) with no 80 MiB prepared index; quality similar (9–11 of 20 hand-picked targets). Results in `docs/spike/fuzzy-bench.md`. Not measured: nucleo's threaded incremental `Nucleo`, which Ctrl+K would use.
- 2026-10-14 — S3 done (orchestrator, Opus): `--rows 100k` gives every pane exactly 100,000 rows; a visible vertical scrollbar (`ScrollbarMode::Always`, 16 px gutter); rows opt in to observation with `test_support()`. Five headless tests (`#[gpui_kit::test]`, `test-support`) pass: 100k rows per pane, End reaches row 99,999 and Space marks it (row visible), Insert marks and moves, PageDown/Home, Tab switches panes. Enabling `test-support` needed a fresh `Cargo.lock` (gpui-fast pins `cocoa =0.26.0`). The manual smoothness check moves to S4's frame-time numbers and the owner's run in S7.
- 2026-10-14 — Incident: the headless Hyprland output used for screenshots broke the owner's Caelestia bar; it was removed at once. No more outputs or windows on the owner's desktop: screenshots need a nested headless compositor (`cage` or `sway`, not installed) or the owner's say. `s3-100k-light.png` predates the scrollbar gutter.
- 2026-10-14 — The owner installed `cage`. GUI runs and screenshots now happen in headless cage (`WLR_BACKENDS=headless`, 1280×720, grim inside the session); Hyprland stays untouched (checked: one monitor, same clients). `s3-100k-light.png` replaced with a cage capture that shows the scrollbar gutter.
- 2026-10-14 — S4 done (ui-builder, Sonnet, medium): `--metrics <path|->` writes JSON (first frame, start-up marks, 10 s auto-scroll frame intervals, RSS after 5 s idle, binary size, GPU name); `metrics.sh` runs N times; 6 unit tests plus the 5 headless ones pass. First numbers, Linux dev PC, headless cage, Intel Arc iGPU, release without LTO (`docs/spike/metrics/linux-cage-*.json`): first frame 670 ms cold, 120 ms warm; RSS 92 MB with three panes of 100k rows and 90 MB with the sample rows (target < 80 MB with two panes); binary 39.3 MB (27.8 MB with fat LTO per D27). Frame intervals are ~39 ms p50 with 100k rows AND with 7 sample rows, and 44 ms with the pixman renderer: headless cage paces frames itself, so its scroll numbers say nothing about the list. Smoothness must be measured on a real display: a Windows PC (S7) and, if the owner allows, one run on the Linux desktop.
- 2026-10-14 — S6 done (ui-builder, Sonnet, high): each list is `Role::List` named "<folder> files"; rows are `Role::ListItem` named "name, size, date" ("src, folder" for folders), `aria_selected` = marked, position/size in set (n of 100,000), and the cursor row is the active descendant. Headless test reads the nodes through gpui-kit's `ElementSnapshot`; 12 tests pass after merging S4 and S6. Limits for G2: GPUI builds the AccessKit tree only while a screen reader is connected (`debug_a11y_tree_json` is `None` headless), so the active descendant and real AT-SPI/UIA output need a manual check with Orca or Narrator; only visible rows exist in the tree.
- 2026-10-15 — S7 CI part done: `spike-p2` run 38622266071 (`03e54c6`) green on windows-latest — 12 tests pass, fat-LTO exe 18.4 MB, three `--metrics` runs saved as `docs/spike/metrics/windows-ci-run-*.json`. On the runner's software renderer (Microsoft Basic Render Driver): first frame 510 / 274 / 220 ms, working set 63 MB (peak 67 MB) with three panes of 100k rows, scroll p50 64 ms (software rendering, not a smoothness verdict). S7 stays `running` until the owner's Windows runs.
- 2026-10-15 — D27 adapter action done: the spike vendors `gpui-pre-windows` 0.3.9 (`spikes/gpui-b/vendor/`, `[patch.crates-io]`) and `get_adapter` enumerates with `EnumAdapterByGpuPreference(MINIMUM_POWER)`; `NAVO_GPU=default|high-performance` overrides it for A/B runs. CI run 38677189477 green (exe 17.6 MB); the runner has only the Basic Render Driver, so its numbers (first frame 546 / 313 / 308 ms, working set 59–60 MB) say nothing about the GPU choice. The Windows run decides.
- 2026-10-15 — Linux memory broken down (headless cage, non-LTO build; `docs/spike/metrics/linux-cage-memory-*.txt`): RSS 86 MB = 27 MB private (anon) + 59 MB file-backed code. File-backed: the 39 MB non-LTO binary (25 MB resident), Mesa's `libLLVM` (15 MB) and `libgallium` (8 MB). 100k rows per pane add ~1.5 MB. `gpui-pre-wgpu` 0.3.9 creates its wgpu instance with `VULKAN | GL` hard-coded (`wgpu_context.rs:474`, ignores `WGPU_BACKEND`), so the GL backend loads gallium and LLVM even though rendering uses Vulkan: ~24 MB of shared, clean pages. For G2: measure the target as private memory (Linux anon RSS, Windows private bytes), which is 27–29 MB on Linux; patching the instance to Vulkan-first would drop RSS to about 62 MB on Linux (D30: Linux is secondary).
- 2026-10-15 — G2 passed. The owner found scrolling smooth on the 60 Hz Windows monitor and agrees with GO for GPUI (D2 validated). Narrator read nothing on the list; the owner does not need a screen reader now, so it becomes a follow-up, not a blocker. The owner also asked for a title bar that looks like the app (D36, P7).
- 2026-10-17 — Statuses closed: S5 (docs/spike/fuzzy-bench.md), S7 (owner's Windows run, 2026-10-15) and G2 (GO, logged 2026-10-15) were done but still marked open.
