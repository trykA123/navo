# Dependencies on their latest releases

Status: ready · Repo: `navo` · Branch: one `deps-<batch>` branch per atom from `origin/main` · Started: 2026-11-10

Owner rule (2026-11-10): every crate stays on its latest release. 39 of 105 direct dependencies were behind on 2026-11-10. Three researchers read the changelogs between the locked and the latest version; their findings set the batches below.

## Decisions

- D1 — **Batches by risk.** Patch bumps with no API change go in one atom. API changes are split by area, so a break stays small and reviewable.
- D2 — **Security first.** `gix` 0.90 fixes GHSA-q54r-jf2p-rfr2 (config trust for path protections). It goes first, alone.
- D3 — **Lockfile order.** Every atom edits `Cargo.lock`. Atoms run one at a time, after the speed-checks chain (A1→A2→A7→A8→A9→C1) has released the lockfile for that slot; each starts from the current `origin/main`.
- D4 — **Held.** `windows-core` and `windows-version` 0.100: `windows` 0.100 is not on crates.io yet, so a bump would build two copies. Wait for it. `windows-core` is declared in `navo-shell` and `navo-bench` but no source uses `windows_core::`; DP1 removes those two lines.
- D5 — **Proof.** Each atom: `cargo xtask check` on the touched crates, the full gate on main, and Windows VM CI. Speed claims (similar 2.5–5×, fastcdc 7–14%, toml) are measured with the speed-check benches before and after, not taken from changelogs.

## Atoms

| Atom | Crates (locked → latest) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| DG | gix 0.89→0.90, gix-config 0.62.0→0.62.2, gix-discover 0.57.0→0.57.1 | api-builder | medium | lockfile slot | `cargo xtask check -p navo-git -p navo-app`; exhaustive `match` on gix `Force` fixed; replacement refs default noted | todo |
| DP1 | patch bumps: similar 3.2.1, xxhash-rust 0.8.19, crc32fast 1.5.5, encoding_rs 0.8.44, flate2 1.1.11, memmap2 0.9.12, rustix 1.1.6, libc 0.2.191, lofty 0.25.5, roxmltree 0.21.2, toml 1.2.1, toml_edit 0.25.19, kamadak-exif 0.6.2, pdfium-render 0.9.5, trash 5.3.0, tree-sitter-json5-orchard 0.1.1, tree-sitter-powershell 0.26.6, ts-parser-perl 2.0.5; drop unused windows-core lines | patcher | medium | DG | `cargo xtask check` on every touched crate; compare bench before/after for similar | todo |
| DA | archives and hashes: zip 9.5.0, sevenz-rust2 0.25.0, lzma-rust2 0.25.0 (`LzipStream::new` flag), md-5 0.11 (digest 0.11; keep hex output identical), fastcdc 5.0.0 (even sizes; chunk hashes for odd tails change — confirm nothing persisted depends on them), dua-core 6.1.0, cpp_demangle 0.5.1, similar 2.7→3.2.1 in navo-core (one copy) | api-builder | high | DP1 | `cargo xtask check -p navo-fs -p navo-compare -p navo-core -p navo-automotive -p xtask`; archive fixtures incl. a malformed 7z and a zip with a symlink | todo |
| DS | parsers: tree-sitter 0.27.2 (`child_count` u32, lookahead `Option`), tree-sitter-sequel 0.3.12 (ABI 15, highlights), quick-xml 0.42 (UTF-8 reader, `decode()` removed; navo-syntax udl, navo-structured xml), saphyr-parser 0.3.3 (empty block scalar at EOF) | api-builder | high | DA | `cargo xtask check -p navo-ui -p navo-syntax -p navo-structured`; every grammar loads at runtime; highlight snapshot per language | todo |
| DE | ELF and DWARF: object 0.41.0 (newtypes, `StringTable::new`), gimli 0.34.0 (attribute queries without `Result`) | api-builder | high | DS | `cargo xtask check -p navo-automotive -p navo-ui`; ELF fixtures incl. long symbol names | todo |
| DU | UI: gpui-kit 0.9.1 + gpui-pre-windows 0.3.12 (pinned together; key, scroll and theme API changes), resvg 0.49 (new text stack) | senior | high | DE | `cargo xtask check -p navo-ui -p navo-app -p navo-ui-gallery`; gallery screenshots both themes; VM tester pass on keys, scrolling and SVG preview | todo |
| DW | windows-core / windows-version 0.100 with `windows` 0.100 | patcher | medium | `windows` 0.100 on crates.io | VM CI | blocked |

## Log

- 2026-11-10 — Plan written from three researcher reports (compare/fs/archives, Git/UI/platform, parsers). MSRVs top out at 1.95; the toolchain is 1.99.
- 2026-11-10 — DS split. tree-sitter 0.27.2 cannot resolve: gpui-component 0.7.3 needs tree-sitter ^0.26.13 and tree-sitter's native `links` forbids two copies. **DS-a** (quick-xml 0.42, saphyr-parser 0.3.3, sha2 0.11 to unify digest 0.11 with md-5) runs now; **DS-b** (tree-sitter 0.27.2, tree-sitter-sequel 0.3.12) runs after DU if gpui-kit 0.9.1 moves to tree-sitter 0.27, else waits for gpui-component. DA merged (00119730).
