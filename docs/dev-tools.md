# Development tools

Versions are pinned to the latest stable release at the time each tool was added. A monthly atom updates them.

| Tool | Version | Installed from | Used for |
| --- | --- | --- | --- |
| Rust toolchain | 1.99.0 | rustup / Arch `rust` | build, fmt, clippy |
| cargo-nextest | 0.9.148 | crates.io (`cargo install --locked`) | test runner |
| typos-cli | 1.51.1 | crates.io (`cargo install --locked`) | spelling |
| cargo-deny | 0.20.2 | Arch package | licences, advisories, sources |
| cargo-audit | 0.22.2 | Arch package | security advisories |
| cargo-llvm-cov | 0.9.1 | Arch package | coverage |
| actionlint | 1.7.12 | Arch package | workflow lint |
| taplo | 0.10.0 | Arch package | TOML check |

| Crate | Version | Used by |
| --- | --- | --- |
| toml | 1.1.8 | `xtask` |
| serde (derive) | 1.0.229 | `xtask` |
| serde_json | 1.0.154 | `xtask` |

`vendor/gpui-pre-windows` is gpui-pre-windows 0.3.9 with Navo patches (low-power GPU, drop target that reports keys and allowed effects, refuses targets without an app hint, and posts the native drag-source hook); its `src/gpui_windows.rs` header lists them.

## Coverage without rustup

On a system Rust without rustup (for example Arch's `rust` package), `cargo llvm-cov` needs `LLVM_COV=/usr/bin/llvm-cov` and `LLVM_PROFDATA=/usr/bin/llvm-profdata`. The system LLVM must match `rustc -vV` (LLVM 23.1.1 today). CI uses rustup and needs neither.

The drag bridge is wired through `StartOptions::drop_platform`. Windows registers the existing shell adapter before opening the window. Incoming sources are frozen without filesystem reads during hover. The app checks the source's allowed effects before planning a job; a right-button drop snapshots those permissions before showing the choice menu.

`navo-ui-gallery --screenshot <dir> --pages p36-x3-drop-menu` captures the shared drop-menu items in both themes. GPUI's `TestAppContext::test_window` is private, so downstream tests cannot set native handoff acceptance or inspect its payload log. The row resolver, session cleanup, reentry and refresh are covered locally; actual OLE promotion, Explorer effects and source deletion behavior require Windows.
