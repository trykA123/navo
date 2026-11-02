# GPUI spike results (P2, G2)

Status: **G2 passed (2026-10-15): GO for GPUI (D2)**. Narrator read nothing on the list; it is a follow-up the owner does not need now; the owner found scrolling smooth on the 60 Hz Windows monitor (2026-10-15); the owner decides go or no-go for D2. Windows PC: Intel UHD + NVIDIA T1200 laptop, fat-LTO spike exe from CI run 38677189477, low-power GPU patch.

Spike: `spikes/gpui-b` (design B, variant 1 of D27, three panes of 100,000 rows each, drawer, F-key bar) on `gpui-kit` 0.7.3. Raw numbers: `docs/spike/metrics/`. Plan and log: `plans/2026-10-14-p2-gpui-spike.md`.

## Targets

| Target | Goal | Linux dev PC (headless cage) | Windows CI (software renderer) | Windows PC |
| --- | --- | --- | --- | --- |
| Cold start, first frame | ≤ 400 ms | 671 ms (non-LTO, first run after build) | 510–546 ms | **407 ms** (Intel, first run); 561 ms on NVIDIA |
| Warm start, first frame | ≤ 150 ms | **120 ms** | 220–313 ms | 343–346 ms (Intel) — **misses**; needs a resident process |
| Idle memory, two or more panes | < 80 MB | RSS 86–88 MB; **private 27–29 MB** | working set 59–63 MB | working set 91–99 MB on Intel, 130 MB on NVIDIA; private bytes not measured |
| `.exe` size | < 30 MB | 39 MB (non-LTO ELF) | **17.6 MB** (fat LTO) | **17.6 MB** |
| Scrolling 100k rows at 60 Hz | no dropped frames | not measurable: cage paces frames at ~39 ms | not measurable: software renderer | **mixed**: Intel run 3 and NVIDIA hold 60 fps (p95 17.4–17.5 ms, nothing over 33 ms); Intel runs 1 and 2 drop to 45 and 35 fps (p95 ~49 ms) |
| Fuzzy matching | interactive on 100k names | frizbee: 1.2–2.1 ms per query on 100k basenames; 54–143 ms on 2M paths | — | — |
| Accessibility | list and rows readable | roles, names, position and selection set (headless test) | — | Narrator check pending |

### Windows PC start-up breakdown (ms from `main`)

| Run | GPU | application created | run callback | open_window returned | first frame |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Intel UHD | 191 | 270 | 405 | 407 |
| 2 | Intel UHD | 161 | 231 | 343 | 346 |
| 3 | Intel UHD | 158 | 229 | 341 | 343 |
| NVIDIA | T1200 | 156 | 406 | 558 | 561 |

- The low-power GPU preference works: device creation on Intel takes ~70 ms against ~250 ms on NVIDIA, which saves ~200 ms of start-up.
- Of the remaining ~340 ms, ~160 ms is the platform and DirectWrite before the app's code runs, and ~105 ms is the window and the first render. A warm start under 150 ms is not reachable by trimming; it needs the app to stay resident (tray) and show a hidden window, as Total Commander and Everything do.
- Scroll frame counts in 10 s: 447, 352, 601 (Intel) and 602 (NVIDIA). The two slow Intel runs show frames at 33 and 50 ms (whole missed vsyncs), not a slow renderer: the same build holds 60 fps in run 3. Likely causes: iGPU power-state ramp-up or another app on screen; to confirm with a longer run.

## Findings

### Start-up

- On a Windows PC the first frame is dominated by D3D11 device creation on the NVIDIA GPU (250–310 ms). On the Intel GPU it is ~70 ms, which brought the variants prototype from ~600 ms to 376 ms.
- The spike now prefers the low-power GPU (vendored `gpui-pre-windows`, `EnumAdapterByGpuPreference(MINIMUM_POWER)`); `NAVO_GPU=default` restores GPUI's order for comparison.
- The other start-up phases on Windows: platform and DirectWrite ~170 ms, window and first render ~130–145 ms. The warm target (≤ 150 ms) likely needs a resident (tray) process on Windows; to be decided after the Windows run.

### Memory

- Linux RSS of 86 MB is 27 MB private memory plus 59 MB of mapped, shared code: the binary (25 MB resident) and Mesa's GL stack (`libLLVM` 15 MB, `libgallium` 8 MB).
- The GL stack is loaded for nothing: `gpui-pre-wgpu` 0.3.9 creates its wgpu instance with `VULKAN | GL` hard-coded (`wgpu_context.rs:474`). A Vulkan-first patch would save ~24 MB of RSS on Linux.
- 100,000 rows in each of three panes add ~1.5 MB.
- Proposal: measure the memory target as **private memory** (Linux anonymous RSS, Windows private bytes), not RSS or working set, which count shared code pages.

### Look and behaviour

- Design B is reproduced at 1440×900 in both themes (`spikes/gpui-b/screens/`), with a visible scrollbar, three panes, tabs, path bars, the drawer (Transfers, Clipboard, Search) and the F-key bar.
- Keyboard: Up, Down, PageUp, PageDown, Home, End, Space, Insert and Tab work on 100,000 rows, checked by five headless tests.
- `gpui-kit`'s headless test support (`#[gpui_kit::test]`, `test-support`) is enough for UI tests in CI.

### Accessibility

- Each list is `Role::List` with a name; rows are `Role::ListItem` with "name, size, date", selection state and position in set.
- GPUI builds the AccessKit tree only while a screen reader is attached, so the real UIA output needs the Narrator check.

### Fuzzy matching

- frizbee 0.13.0 is 2–4 times faster than nucleo-matcher 0.3.1 and needs no prepared 80 MiB index; quality is similar (9–11 of 20 hand-picked targets). See `fuzzy-bench.md`.

## Decision

Proposed: **go for GPUI (D2), with three follow-ups.**

1. Cold start meets the target on a Windows PC thanks to the low-power GPU patch; offer the patch upstream and keep it vendored until then.
2. Warm start needs a resident mode: Navo stays in the tray after its window closes and shows a pre-built hidden window (a plan item for after PoC 2).
3. Measure private bytes on Windows (the P5 memory probe in `navo-app`) and a 60-second scroll on a Windows PC, to settle the memory target and the Intel frame drops.

Owner's view of smoothness (2026-10-15): scrolling feels smooth on the 60 Hz monitor.

Decision (2026-10-15): the owner agrees with GO. Narrator read nothing on the spike list; fixing it is a follow-up (the owner does not need a screen reader now).
