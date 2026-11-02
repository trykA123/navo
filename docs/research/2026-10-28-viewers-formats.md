# Viewers and file formats: crate research (2026-10-28)

Versions and dates come from the crates.io API on 2026-10-28. Navo uses gpui-kit =0.7.3. Its 0.8.1 (2026-10-26, Apache-2.0) pins `ropey =2.0.0-beta.1`, `lsp-types` and `gpui-pre-sum-tree`, so a rope editor is already in our tree.

| Need | Candidate (version, date) | Licence | Windows | Verdict and reason |
|---|---|---|---|---|
| Highlighting | tree-sitter-highlight 0.27.1, 2026-10-08 | MIT | yes | ADOPT for new viewers; same stack as gpui-kit's editor |
| Highlighting | syntect 5.3.0 (2025-09-27) + two-face 0.5.2 | MIT | yes | TRY as fallback for languages with no grammar (gpui-kit has a `syntect` feature, about 200 languages) |
| User grammars | tree-sitter `wasm` feature (wasmtime-c-api) | MIT | UNVERIFIED | TRY a spike: sandboxed `.wasm` grammar + `highlights.scm` packages (Zed's extension model) |
| User grammars | tree-sitter-loader (native DLLs) | MIT | yes | SKIP: unsandboxed code, CrowdStrike risk |
| Rope | Zed `rope`, `text`, `multi_buffer`, `language` | GPL-3.0-or-later | yes | SKIP: licence. `sum_tree` is Apache-2.0 but transitive only |
| Large files | memmap2 0.9.12, 2026-10-21 | MIT/Apache | yes | TRY for compare/index only; a file that shrinks while mapped raises an in-page I/O error on Windows. Keep `read_at` paging for F3 |
| Editor widget | gpui-kit code editor 0.8.1 (README: "stable at 200K lines") | Apache-2.0 | yes | ADOPT for F4; test a 1M-line file. SKIP gpui-editor 0.2.0 (53 downloads) |
| Spreadsheet read | calamine 0.37.0, 2026-10-18 | MIT | yes | ADOPT. xlsx/xlsb load lazily per sheet; xls/ods load all sheets (README), so cap their size |
| Spreadsheet write | rust_xlsxwriter 0.101.0, 2026-10-20 | MIT/Apache | yes | ADOPT for "save as xlsx" |
| Thumbnails | fast_image_resize 6.1.0, 2026-07-21 | MIT/Apache | yes | ADOPT: SIMD resize for thumbnail grids |
| JPEG thumbs | turbojpeg 1.5.1, 2026-07-25 | Unlicense/MIT | C build | TRY: scaled decode (1/2 to 1/8); benchmark against zune-jpeg |
| AVIF | image `avif-native` (dav1d, mp4parse); rav1d 1.1.0 (2025-05-07) | BSD-2/MPL-2 | C build for dav1d | TRY. `image`'s `avif` feature is encode only (ravif) |
| HEIC | libheif-rs 3.0.0 (MIT wrapper), libheif is LGPL; heic 0.1.6 is AGPL | LGPL/AGPL | vcpkg | SKIP static link. Use WIC "HEIF Image Extension" (Store download, may be missing) |
| RAW | rawler 0.8.0, rawloader 0.37.2, quickraw | LGPL-2.1 | yes | SKIP: licence. Show the embedded JPEG preview via `tiff` 0.11.3 (MIT) + kamadak-exif |
| Video/Office/HEIC/RAW thumbs | Windows `IShellItemImageFactory::GetImage` (windows crate, Vista+) | OS | yes | ADOPT in a helper process (shell extensions are third-party code) |
| Video thumbs (Linux) | ffmpeg-sidecar 2.7.0, 2026-10-18 + ffmpeg exe | MIT wrapper; ffmpeg LGPL/GPL, separate process | yes | TRY later. IPC cost: one process start per file |
| Office text | own zip + quick-xml | ours | yes | KEEP OURS (about 200 lines). SKIP undoc 0.27.1, docx-lite 0.2.0, pptx-to-md 1.0.0 (young or small) |
| EPUB | epub 2.1.5 is GPL-3.0; rbook 0.7.12, 2026-10-10 | Apache-2.0 | yes | SKIP epub. TRY rbook, else zip + quick-xml |
| MF4 | mf4-rs 3.8.0 (2026-10-28, mmap, lazy); mdf4-rs 0.6.0 (2026-07-20) | MIT; MIT/Apache | yes | TRY mf4-rs. README: compressed `##DZ` blocks not supported. Both are young; mdf4-rs DZ support UNVERIFIED |
| ARXML | autosar-data 0.23.0, 2026-08-15 | MIT/Apache | yes | ADOPT |
| ODX/PDX | odx-rs 0.39.0, 2026-08-08 (844 downloads) | MIT | UNVERIFIED | TRY after code review; else quick-xml tree |
| Binary templates | binrw 0.15.2, 2026-07-23 | MIT | yes | TRY for built-in hex-view structure overlays (PE, ELF headers) |

## Notes

- Zed ships extension grammars as `.wasm` under wasmtime (tree-sitter `wasm` feature). UNVERIFIED: binary size and start-up cost; measure in the spike.
- KEEP OURS (no change): paged `ByteSource`/`LineIndex`, navo-syntax lexer, hex view, Intel HEX/S-record, csv, DBC (can-dbc), Office text. `image` already gives zune-jpeg and image-webp.
- No Go helper beats Rust here.

## Top 5 picks, in order

1. calamine (xlsx/xls/ods in F3) plus rust_xlsxwriter for export.
2. Shell/WIC thumbnails in a helper process (video, HEIC, Office, RAW; no new licences).
3. fast_image_resize in the thumbnail path; benchmark turbojpeg scaled decode.
4. autosar-data for ARXML, then mf4-rs for MF4.
5. tree-sitter wasm grammar-plugin spike, keeping gpui-kit's rope editor for F4.

## Sources

- crates.io API per crate (https://crates.io/crates/<name>), 2026-10-28.
- https://github.com/zed-industries/zed (crates/rope, text, language: GPL-3.0-or-later; sum_tree: Apache-2.0)
- https://github.com/longbridge/gpui-component; https://github.com/tree-sitter/tree-sitter (lib/Cargo.toml)
- READMEs: tafia/calamine, dmagyar-0/mf4-rs, DanielT/autosar-data, strukturag/libheif, Cykooz/libheif-rs
- https://learn.microsoft.com/en-us/windows/win32/wic/heif-codec
- https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ishellitemimagefactory
