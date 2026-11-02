# P16 · Automotive pack: viewers for embedded and vehicle-network files

Status: planned (after P15 A1) · Repo: `<repo>` · Branch: `p16-automotive` · Started: —

## Goal

The owner works in automotive and opens large embedded and vehicle-network files every day ("m-ar ajuta si extensii pentru mamutii astia… .map, .hex, .elf"). After P16, F3 shows the meaning of these files, not just their bytes:
- memory maps of HEX and S-record images;
- the sections and symbols of an ELF;
- who uses the flash in a linker map;
- the signals in a DBC or A2L;
- a CAN trace as a filterable table.

Every view works on files of hundreds of MB without freezing. This is Navo's first real module pack ("Automotive", D13), built on P15's viewer kinds.

Input: the crate research of 2026-10-15 (summary under Decisions).

## Decisions

- **D1 — One module, "Automotive".** It registers viewer kinds and languages, and can be turned off like any module. Each kind sniffs by extension and content, so `.hex` that is really Intel HEX opens here, and anything else falls back to text or hex.
- **D2 — Intel HEX and Motorola S-record** (`.hex`, `.ihex`, `.s19`, `.s28`, `.s37`, `.srec`, `.mot`, `.sx`). Our own streaming parser (the format is simple, and the existing crates are from 2020). The view shows:
  - record counts by type;
  - contiguous address ranges and gaps (a memory-map bar);
  - the start address;
  - checksum errors with line numbers;
  - overlaps and a missing EOF record.

  A hex view on real addresses fills the gaps with `--`. The same image can be exported as a `.bin` later.
- **D3 — ELF** (`.elf`, `.axf`, `.out`), with `object` and `gimli` over a memory map. The view shows:
  - the header;
  - sections (address, size, flags);
  - LOAD segments;
  - a searchable and sortable symbol table (value, size, type, bind, section);
  - totals of flash and RAM used;
  - with DWARF, the source file and line for a symbol or address (`addr2line`).
- **D4 — Linker maps** (`.map`). One parser per toolchain feeds a common model: memory regions (origin, length, used, free), output sections, per-object contributions, symbols and discarded sections. Order: GNU ld, IAR, TASKING, Green Hills, Wind River Diab, armlink. The text view keeps colours; the summary view answers "who uses the flash" with a sortable table.
- **D5 — Calibration and description files.**
  - **A2L.** `a2lfile` reads it. Collapsible `/begin` and `/end` blocks; MEASUREMENT and CHARACTERISTIC tables (address, type, conversion, unit); IF_DATA (XCP and CCP).
  - **DCM and PAR.** Rule-lexer colours.
  - **CDFX.** XML tree.
- **D6 — Networks and diagnostics.**
  - **DBC.** `can-dbc` reads it. Tables of nodes, messages (hex ID, DLC, cycle time) and signals (start bit, length, byte order, factor, offset, range, unit), plus value tables.
  - **LDF.** `lin-ldf` reads it. Nodes, frames, signals and schedules.
  - **ARXML.** The P15 streaming XML tree, with an AUTOSAR package-path breadcrumb. `autosar-data` =0.23.0 (MIT or Apache-2.0; D88 picks it) gives the AUTOSAR model; it comes in only if a 100 MB benchmark shows it opens within D10 and stays under its memory cap. Otherwise the streaming tree stays alone.
  - **ODX, CDD and CDFX.** XML tree. `.pdx` opens as an archive.
- **D7 — Bus logs and measurements.**
  - **Formats.** Vector ASC and PEAK TRC (our own text parsers); BLF (our own bounded container reader); MDF4 on `mf4-rs` =3.8.0 (MIT; memory-mapped, lazy; D88 picks it), after a benchmark. `mf4-rs` cannot read compressed `##DZ` blocks yet: such channel groups show "Compressed data is not supported yet" and the rest of the file still opens.
  - **CAN logs** are a virtualised table: time, channel, ID, DLC, data, direction, error frames. Filtering is by ID, with per-ID counts and bus load. Follow mode works on growing ASC and TRC logs.
  - **MDF4** lists channel groups and channels (unit, sample count). Plots come later.
- **D8 — Scripts and sources.**
  - **Tree-sitter grammars:** TRACE32 `.cmm` (`tree-sitter-t32`), GNU linker scripts (`tree-sitter-linkerscript`) and assembly (`tree-sitter-asm`).
  - **Rule lexer:** IAR, NXP and TASKING linker scripts (`.icf`, `.lcf`, `.lsl`), OSEK `.oil` and `.m4`.
- **D9 — VBF.** A small own parser: header fields, then the blocks shown as hex on real addresses.
- **D10 — Performance contract.** Every view opens a 500 MB file within 1 s to first content and stays under 200 MB of memory. Indexes are built in the background with progress, and only the visible part is decoded. Each format gets a benchmark fixture in CI.

- **D11 — Layout: direction B, "inspector" (owner, 2026-10-19).** From the Opus prototype (`<worktrees>/prototypes/automotive/opus`, shots `B-*`): a navigator on the left (ranges, sections, messages or IDs with counts), the content in the middle (hex on real addresses, symbol or signal table, virtualised trace), an inspector on the right for the selected item (size, position, records by type, checks, decoded signals). Text, hex and the rich view stay on Alt+1/3/5. **Navigator and inspector are drawers over the content (owner, 2026-10-19):** the content always keeps the full width; each drawer slides in over it from its side, with the same frame and motion as the preview side sheet, and closes with Esc or its key. Opening the inspector does not move the content's columns.
- **D12 — Simple by default, rich parts opt-in (owner, 2026-10-19).** The owner wants a calm default and full control over the rich parts. The rich view opens in **Simple** mode: navigator as a plain list (name, address, size), content, a one-line status with the checks (checksum errors, EOF, gaps) and no charts. Each rich part is a setting under `[automotive.view]` in `navo.toml`, live like every setting, and shown as a toggle group in the settings sheet's Automotive section:
  - `inspector` — the right-hand drawer opens by itself when an item is selected (default off; the drawer is still available on its key);
  - `memory_strip` — the address-map strip beside the navigator and the position bar (default off);
  - `charts` — bars, sparklines, bus load and signal curves, wherever they appear (default off);
  - `row_bars` — the per-row bars and counts in the navigator (default off);
  - `preset = "simple" | "rich" | "custom"` sets all four at once; changing one toggle makes it `custom`.
  
  In the viewer, `Ctrl+B` opens or closes the navigator drawer and `Ctrl+I` the inspector drawer (D58; `Ctrl+L` is the pane's edit path), for this file only, without changing the setting. A View menu (button at the top right and `Alt+V`, D58) shows Simple / Rich / Custom and the four toggles with their keys; a change applies to the open file at once, and "Keep as default" writes `[automotive.view]`. Problems (checksum errors, missing EOF, gaps) always show in the status line, in every mode. In Simple mode the content is full width and both drawers start closed. Every part works with every combination; the Simple layout is designed and screenshot-tested first, Rich second.

## Atoms (cut into Red/Green pairs when P15 A1 lands)

| Atom | What | Role |
| --- | --- | --- |
| B1 | Module "Automotive", sniffers and file associations | orchestrator |
| B2 | HEX and S-record parser and memory map (D2) | Red checker, Green orchestrator |
| B3 | ELF sections, segments, symbols and DWARF lines (D3) | Red checker, Green general-purpose (Opus medium) |
| B4 | Linker map model and GNU ld parser (D4), then IAR, TASKING, GHS, Diab and armlink | Red checker, Green general-purpose |
| B5 | A2L and DBC tables, LDF (D5, D6) | Red checker, Green general-purpose |
| B6 | ASC, TRC and BLF CAN table with filter and follow (D7). TRC backend done and routed live in the trace view with the format label (Vector ASC or PEAK TRC version); BLF backend done, BLF UI routing pending. | Red checker, Green orchestrator |
| B7a | MDF4 spike: `mf4-rs` =3.8.0 over a 500 MB generated MF4 and the owner's files; open time, memory, a `##DZ` file refused cleanly (D7, D88) | api-builder (Sol 6.1 high or DeepSeek V4.1 Flash) |
| B7 | MDF4 channel browser (D7), after B7a passes | general-purpose |
| B8a | ARXML spike: `autosar-data` =0.23.0 against the streaming tree on a 100 MB and a 300 MB ARXML; open time and memory (D6, D88) | api-builder (Sol 6.1 high or DeepSeek V4.1 Flash) |
| B8 | ARXML breadcrumb (with `autosar-data` only if B8a wins); ODX, CDD, CDFX and PDX wiring (D6) | general-purpose |
| B9 | Grammars and rules for `.cmm`, linker scripts, assembly, `.oil`, `.m4`, DCM/PAR; VBF (D8, D9) | general-purpose |
| B10 | Benchmarks (D10), UI for the summary views | ui-builder-high |
| B11 | Review | reviewer |

## Done when

- A 50 MB S19 file shows its address ranges and any checksum error in under 1 s.
- An ELF shows sorted symbols, and a GNU ld `.map` shows flash use per object.
- A DBC lists messages and signals, and an ASC trace filters by ID.
- A 300 MB ARXML opens as a navigable tree.
- The whole pack turns off in settings.
- The gate, Windows CI and the reviewer are green.

## Log

- 2026-10-20 — B5a backend: pinned can-dbc 10.0.0 and a2lfile 3.5.1; own DBC/A2L table rows, ID lookup, content sniffers, bounded retention, Watch progress/cancellation and up to 1000 line-numbered problems. Parsing individual records/row blocks preserves other rows after errors; A2L includes are reported and left unloaded. Invented release fixtures: A2L 50,000,153 bytes / 98,061 measurements, 0.361 s / 74,584 KiB peak RSS; DBC 5,000,130 bytes / 19,200 messages, 0.345 s / 41,032 KiB peak RSS. A separate dense 50 MB whole-tree a2lfile probe took 0.578 s / 1,154,316 KiB, so the index uses bounded row blocks. Full 500 MB/first-content D10 and UI wiring remain unverified in this backend atom.
- 2026-10-19 — Owner picked direction B (inspector) from the Opus prototypes and asked for a simple default with every rich part switchable in settings (D11, D12).
- 2026-10-19 — Owner: the left and right panels are drawers over the content, not columns that shrink it (D11).
- 2026-10-19 — D58 (owner, whole app): every rich viewer opens calm, uses Ctrl+B / Ctrl+I for its left / right drawers and has a View menu (Alt+V) with Simple / Rich / Custom and per-part toggles; D12 updated (navigator on Ctrl+B, not Ctrl+L).
- 2026-10-19 — Owner's work set: `.elf` (10 MB), `.hex`, `.map` (4.6 MB), `.mot`, a 389-byte `.mdf` (not MDF4 by size; format unknown) and CAN traces in Vector `.asc`. Order after B2: B3 ELF, B4 maps (toolchain to confirm; `.mot` suggests Renesas, whose CC-RH/CC-RL map is not in D4's list yet), B6 with ASC first.
- 2026-10-19 — B3 ELF view landed (symbols, sections navigator, inspector with DWARF lines, Ctrl+G). D10 for ELF is met for the owner's 10 MB files only; the full contract (500 MB within 1 s, under 200 MB of memory, CI benchmark fixture) is left to B10.
- 2026-10-19 — Owner: the work `.map` files come from the TASKING toolchain (likely TASKING VX for TriCore/AURIX). B4 order changes: TASKING map parser first (its `Locate Result` / `Link Result` / `Processed Files` tables), then GNU ld; IAR, GHS, Diab and armlink after.
- 2026-10-20 — B4 map view landed (`p16-b4b`): a `.map` whose head `sniff_map` accepts opens the linker map view on F3 and Alt+5 (any other `.map` stays text). Memory (default), Sections and Symbols tables switch with Alt+T or a click; Ctrl+F3/F4/F5/F6 and header clicks sort; every letter types into the name filter; Ctrl+B lists regions then objects (Enter shows only that region or object); Ctrl+I inspects the row; Ctrl+G finds the symbol holding an address; the status line (toolchain, counts, used and free, problems, skipped parts) is never hidden. Origin shows only when the map gives one (TASKING's Memory Usage does not). Gallery pages and screenshots `p16-b4b-map-*`. Not done: row bars and the memory strip (parts toggle, nothing drawn yet), GNU ld and the other toolchains (D4).
- 2026-10-20 — B6b: TRC 1.1, 1.3, 2.0 and 2.1 backend uses the ASC frame model, sparse line index and Follow path; invented fixtures cover errors, FD, malformed rows and bounded window reads. Release benchmark: 50,000,054 bytes / 1,063,830 rows indexed in 0.101 s. BLF deferred under the atom's stop rule: ablf 0.2.1 streams containers but lacks CAN_MESSAGE(1), CAN_FD_MESSAGE(100), CAN_FD_MESSAGE_64(101), fallible iteration and object offsets; decompression can panic. Adapting it or writing the container/offset reader exceeds half the atom's work. BLF backend, LOGG sniffing and BLF UI remain pending.
- 2026-10-20 — B6c: own BLF backend reads LOGG, v1/v2 LOBJ timestamps, plain/zlib containers with cross-container objects, CAN_MESSAGE, CAN_MESSAGE2, CAN_FD_MESSAGE, CAN_FD_MESSAGE_64 and CAN_ERROR_EXT. The shared trace model keeps sparse container/inner checkpoints every 32 frames, bounded window inflation, per-type skipped counts and partial results on corruption; Follow is disabled. Binary LOGG sniffing is exposed. BLF UI routing must check the magic before binary rejection, dispatch the background index job to index_blf with a seekable source, and disable Follow; TRC/BLF UI work remains separate. Invented fixtures cover all supported objects, every byte split in a v2 FD object, corruption, truncation, allocation bounds and lazy inflation. Release benchmark: 50,143,344 bytes / 1,044,480 frames indexed in 0.056 s; cargo-q xtask gate and workspace Clippy passed (exit 0); 5,292 tests passed, four skipped. The full D10 memory and 500 MB target remains for B10.
- 2026-10-20 — B6d: BLF UI. A Vector BLF opens in the CAN trace view by content (`sniff_blf` runs before the binary rejection, so any extension works); the background index job calls `index_blf` over a seekable ranged source with the same progress and cancellation, and windows decode through the existing Plan path. The header names Vector BLF (`trace-format-blf`); Alt+F shows a note and the Follow key hint is hidden, since `supports_follow()` is false. A text file starting with `LOGGING` stays text. Tests in `tests/can_trace_blf.rs` build an invented BLF from bytes. No gallery BLF variant: the trace pages are screenshot scenes over ASC fixtures and a BLF one is not cheap.
- 2026-10-20 — TASKING wrapped rows (`p16-map-wrap`): TASKING wraps at word boundaries as well as inside words, so a line continues the previous logical row when it is a `|` row with the same cell count, is not a separator and cannot be a row on its own: a column that must hold text in that table is empty (Link Result: size, offset and address columns; Locate Result: Chip and Size; Removed Sections: File; Symbols: address and value). Each non-empty continuation cell joins its cell, without a space when the previous piece filled its column (at most one trailing space before the `|`) and with one space otherwise; cells wrap over several lines, and the first continuation cell may be non-empty (Removed Sections has no separators). A full row whose text cell fills its column exactly never merges with the next full row. Problems report the first line of the logical row. Invented fixtures plus the owner's real map: 5,169 problems before, 0 after (Removed Sections 825 rows before, 621 after the wrapped ones joined).
- 2026-10-29 — Amended from the 2026-10-28 research (D88): ARXML uses `autosar-data` 0.23.0 and MDF4 uses `mf4-rs` 3.8.0 (`falcon_mdf` dropped), each only after a spike. Added B7a (MDF4 spike, compressed `##DZ` blocks refused cleanly) and B8a (ARXML spike against the streaming tree). Spikes may run on Sol 6.1 high or DeepSeek V4.1 Flash (backend only).
