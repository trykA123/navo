# P31 · Per-monitor DPI and window placement

Status: ready · Repo: `<repo>` · Branch: `p31-dN` per atom · Started: 2026-10-19

## Goal

Navo is sharp on every monitor at any scale, follows the scale when the window moves between monitors, and opens where it was closed: per monitor setup, so a laptop docked at the office and the same laptop alone each get their own place. Architecture: "Per-monitor DPI: each monitor keeps its own scale; window placement is saved per monitor setup. Alpha."

Found on 2026-10-19 (scout): neither the vendored GPUI nor `navo-app` declares DPI awareness (`build.rs` embeds icon and version only, no manifest), so Windows probably bitmap-scales the window on scaled displays and never sends `WM_DPICHANGED`, although GPUI handles that message (`vendor/gpui-pre-windows/src/events.rs` `handle_dpi_changed_msg`). The window always opens centred at 1280 × 800 (`navo-ui/src/window_view/start.rs`); `WindowState` in the session holds no bounds.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Manifest.** `navo-app` embeds an application manifest through the existing `embed-resource` step: `dpiAwareness` `PerMonitorV2, PerMonitor`, `dpiAware` `true/pm` for older systems, and the Windows 10/11 `supportedOS` id. Nothing else changes in the manifest (no long-path or code-page entries).
- **D2 — Evidence.** The diagnostic report's system section adds the process DPI awareness (`unaware`, `system`, `per-monitor`, `per-monitor-v2`) and fills `display scale` from the live window when one exists (today the no-window report writes 0.0). A Windows CI test runs the built exe with `--diag-report` and asserts `per-monitor-v2`.
- **D3 — Placement model.** The session's window state gets `placement: Vec<Placement>` (at most 8, most recent first). A `Placement` holds a monitor-setup fingerprint (sorted monitor rectangles in physical pixels with each scale), the window's normal bounds in logical pixels with the monitor it was on, and `maximized`. Old sessions without it load as empty (session migration test).
- **D4 — Restore.** At start: a placement whose fingerprint equals the current setup is restored exactly (maximized on its monitor if it was). Otherwise the newest placement is used if at least a quarter of its window lies on a current monitor's work area, moved fully inside that work area and shrunk to fit; otherwise the window opens centred on the primary monitor at 1280 × 800 or 80 % of the work area, whichever is smaller. Saving happens with the session (on quit and the existing periodic save), never on every move.
- **D5 — Out of scope.** Restoring several windows (Navo has one), per-monitor font sizes, and snapping layouts.

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| D1 | D1 manifest and D2 report field and CI test | api-builder (Codex sol high) | high | — | Windows CI: `--diag-report` shows `per-monitor-v2`; Linux build unchanged; gate | done |
| D2 | D3 placement model in the session with migration, and the pure D4 restore function (setup, placements → bounds and maximized) | api-builder (Codex sol xhigh), then reviewer (Opus) | xhigh | — | session tests: round trip, old session loads, cap 8; restore tests: exact match, moved monitor, smaller monitor, monitor gone, maximized; proptest: the result always lies inside a current work area; gate | done |
| D3 | Wire D2 into the window: read displays and bounds from GPUI at save, apply at open | ui-builder (Opus medium) | medium | D1, D2 | UI test with a fake display list; owner check on a Windows PC: two monitors at different scales, sharp text on both, close and reopen in place, undock and redock | done |
| D4 | Review | reviewer (Opus) | medium | all | verdict | ready |

## Done when

- On the owner's PC the window is sharp at 100 %, 125 % and 150 %, follows the scale when dragged across monitors, and reopens where it was for each monitor setup; the gate and Windows CI pass.

## Log

- 2026-10-19 — Plan written after a scout of DPI and placement (no manifest, no saved bounds).
- 2026-10-19 — Correction: Navo is already per-monitor-v2 aware. `gpui-pre`'s default `windows-manifest` feature embeds `gpui.manifest.xml` (PerMonitorV2, `true/pm`, Windows 10 supportedOS, Common Controls 6, SegmentHeap); the scout searched only `vendor/`. A second manifest fails the link (CVT1100 duplicate resource). D1 shrinks to the evidence part: the report's `dpi_awareness` field and the Windows CI test that `--diag-report` says per-monitor-v2.
- 2026-10-19 — D2 merged after review fixes: session format v2 (`placement` list, v0/v1 load with none; an older build falls back to the v1 `.bak`); exact matches are still moved inside the current work area (taskbar moves); `Monitor.primary` with origin and first as fallbacks; a frozen v1 session file written by the old code is loaded unchanged in a test.
- 2026-10-19 — D1 merged: the report's `dpi_awareness` field; Windows CI confirms per-monitor-v2 through `navo --diag-report`. D3 running (Opus medium).
- 2026-10-19 — D3 merged (Windows CI green): GPUI gives display bounds and visible bounds but no scale, so `navo_shell::display_scale` reads GetDpiForMonitor per display; placement is saved on close and quit (there is no periodic session save); any failure opens centred at 1280×800. Owner check pending: second monitor at another scale, maximized, undock/redock.
