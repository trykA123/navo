# P28 · Start-up and memory on real hardware

Status: parked (owner, 2026-10-19: features first, then optimise) · Repo: `<repo>` · Started: 2026-10-19

## Goal

Meet the v1.0 targets on the owner's PC (cold ≤ 400 ms, warm ≤ 150 ms, RAM < 80 MB). Numbers and phases are in `docs/perf.md` ("Owner's PC").

## Findings (2026-10-19, build 5844556, Intel UHD Graphics)

- Process start to `main`: 76–739 ms (CI 16 ms). Suspect antivirus scanning an unsigned 89 MB exe on each launch; the exe is large mostly because ~50 tree-sitter grammars are compiled in. Owner confirmed (2026-10-19): a work laptop with corporate antivirus and CrowdStrike, where every program loads slowly; so part of the pre-`main` time is outside Navo's control, and a smaller, signed exe is the lever.
- GPUI start (window opening → app running): 212–248 ms (CI 62 ms): D3D device, DirectComposition, DirectWrite fonts.
- Window build: 70–97 ms; databases and settings ≈ 45 ms.
- Private bytes ≈ 140 MB with a real GPU (CI 67 MB with the software renderer); ours versus the driver's is unknown.

## Next steps when unparked

1. Measure: marks inside GPUI's Windows start (vendored `gpui-pre-windows`), a heap-bytes counter in `--metrics` to split our memory from the driver's; the owner reruns `--metrics`.
2. Then, by the data: grammars loaded on demand (smaller exe), deferred font loading, signed builds (already a v1.0 item).

## Log

- 2026-10-19 — Owner's `--metrics` runs recorded; plan parked at the owner's request until the alpha features are in.
