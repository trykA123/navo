# gpui-pre-windows: local patches

This folder is a vendored copy of the GPUI Windows platform layer. The workspace patches it in at the root `Cargo.toml`.

## Upstream

- Crate: `gpui-pre-windows` 0.3.12 from crates.io (see `Cargo.toml`).
- Source repository: `https://github.com/zed-industries/zed`.
- Upstream commit: not recorded. The published crate has no `.cargo_vcs_info.json`, and no Navo commit names one.
- First vendored at 0.3.9; rebased onto 0.3.12 on 2026-11-10.

To list the current patches, compare this folder with the unpacked 0.3.12 crate from the cargo registry.

## Patches

One line per patch: the files it changes, what it does, and why.

- `directx_devices.rs`: prefer the low-power GPU; `NAVO_GPU` (or the legacy `FERRITE_GPU`) set to `default` or `high-performance` overrides it. Reason: on a laptop with two GPUs, the device starts about 200 ms faster on the integrated GPU (spike results).
- `window.rs`: the drop target stores the held keys, lets the app pick the effect (none without an app hint), never returns a move effect from Drop, and exports the drop effect, held keys and allowed effects. Reason: Navo shows only the effects the source allows and never lets Explorer delete files Navo moves itself.
- `window.rs`, `events.rs`: an external drag handler registers a native drag source; a posted message starts it outside input dispatch, then sends the drag-ended event. Reason: drag files out of Navo into Explorer.
- `vsync.rs`, `vsync_park.rs`, `platform.rs`, `events.rs`, `dispatcher.rs`, `directx_renderer.rs`, `window.rs`: the VSync thread parks while every window is hidden, minimised or idle; window messages, dispatches, redraws, presents and drops wake it; a park lasts at most 250 ms, and the first frame after a park sleeps one interval. Reason: less idle CPU, and a big folder no longer stays blank after opening.
- `window.rs`, `events.rs`: a resizable pop-up window gets edge sizing without a visible frame. Reason: the clipboard pop-up can be resized by its edges.
- `platform.rs`: since 0.3.12 key downs skip the window procedure, so accelerator translation also wakes the VSync thread. Reason: keep the VSync park working after the 0.3.12 rebase.
- `keyboard.rs`: the keyboard mapper returns bindings unchanged when key equivalents are off. Reason: keymaps stay independent of the keyboard layout, as they were before 0.3.12.

The header of `src/gpui_windows.rs` lists the patches in short form; it does not yet name the pop-up and keyboard patches.
