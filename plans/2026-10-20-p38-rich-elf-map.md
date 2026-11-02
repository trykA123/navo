# P38 · Rich ELF and MAP views: address space first

Status: ready for owner review (Q1–Q3) · Repo: `<repo>` · Branch: `p38-<atom>` per atom · Started: 2026-10-20

## Goal

Today the Rich and Simple modes of the ELF and MAP viewers look the same: the Rich parts were never built. After P38, Rich shows where the memory goes. Each region gets a used and free bar and a memory strip in address order. A treemap shows who uses the flash, with the table beside it. Every ELF works, whatever its toolchain or layout: per-function sections, merged sections, stripped files, with or without a `.map` or `.mdf` beside it. Tables in ELF, MAP, CAN trace and CSV views get resizable columns and names that keep their prefix. HEX and S-record views fold padding gaps and use 32 bytes per row on wide screens. A TASKING `.mdf` opens as a table. Simple stays as it is today.

## What exists today (checked at 9636e33)

- **Parts and View menu.** `AUTOMOTIVE` and `AUTOMOTIVE_TABLE` part sets (`viewer/parts/mod.rs:82-117`) with the fields Inspector, Memory strip, Charts and Row bars (`settings/fields.rs:93-97`, `settings/viewer_fields.rs:62-110`). ELF and MAP use `AUTOMOTIVE_TABLE`, so their View menu lists only "Open the inspector". `show_parts` passes only `opens_inspector` to the bodies. No body draws a strip, a bar or a chart.
- **ELF model** (`navo-automotive/src/elf/mod.rs`). `Elf { header, sections, segments, symbols, problems, totals }`. `Section` has `kind`, `flags`, `address`, `size`, `offset`; `Symbol` has `value`, `size`, `kind`, `bind`, `place`. `Lines` maps addresses to source lines. `Query`/`select` sort and filter the symbol table; there is no noise filter.
- **ELF fixture builder.** `src/elf/sample/mod.rs` (`Spec`, `text`, `data`, `bss`, `func`, `object`, `line_program`, `build`) builds 32/64-bit, little/big-endian ELF with or without symbols and DWARF. `tests/elf_large.rs` reads a 10 MB, 55,000-symbol file.
- **MAP model** (`src/map/model.rs`). `MapModel { regions, sections, contributions, symbols, discarded, problems, skipped_sections }`. Only the TASKING flavour exists. `MemoryRegion.origin` is `None` for TASKING (its Memory Usage table has no origin).
- **Tables.** ELF (`elf/draw.rs:16-60`), MAP (`map/draw.rs:16-45`, heads in `map/select.rs:35-105`) and CSV (`table/draw.rs:16-170`) each copy `CHAR_WIDTH = 7.2` and fixed character widths. The CSV view computes content widths (`navo-structured` `column_widths`). The CAN trace draws each row as one formatted string (`trace/text.rs:16-21`, `138-160`). No column can be resized.
- **HEX/MOT.** `Layout` (`src/layout.rs`) has a fixed `ROW_BYTES = 16`. Every gap between two ranges becomes a gap row.
- **Sibling files.** `FileHooks.bytes` and `FileHooks.range` (`viewer/files.rs:108-128`) read any `Location`; `Location::parent` and `Location::join` build a sibling path.
- **Text.** New UI text goes into `.ftl` files (D59): `navo-l10n/locales/en-US/viewer.ftl` and `settings.ftl`.

### Measured on the owner's files (counts only; the files stay private)

| Fact | First ELF (10 MB) | Second ELF (26 MB) |
| --- | --- | --- |
| Sections | 3,309, one per function or object | 112, merged output sections plus many small NOBITS areas |
| LOAD segments | 3,294 | 51 |
| Symbols | 59,494 | about 99,700 |
| Zero-size symbols | 56,910 (96 %) | about 94,700 (95 %), mostly LOCAL compiler labels typed FUNC |
| ABS symbols | 3,509 | — |
| Sized, not ABS | 2,538 | about 5,000 |
| FILE symbols | name compiler temp files | about 600, compiler temp `.src` files |
| File bytes | loaded 0.19 MB, DWARF 5.5 MB, one note section 3.0 MB, symbols and strings 1.6 MB | — |
| Parse time (release) | — | symbols 6 ms, DWARF lines 22 ms |

- **MAP (4.8 MB, TASKING).** The Symbols tables have three columns: Name, Space addr, Space. There is **no size column**. The Space cell is printed on the first two rows of about 1,700 and is blank afterwards: TASKING prints the space only when it changes.
- **"N other parts not shown".** `parts::header` treats every line that starts with `*` as a part header. The call-graph part has 955 lines of the form `* name [n,m]`, so the count is near 965. The real unread parts are six to eight titled headers.
- **HEX.** 198 ranges with 196 gaps of 16 bytes or less and one gap over 64 KiB.
- **`.mdf`** (389 bytes). TASKING memory definition, plain text: `space <name> <id> <mau>`, `chip <name> <id> <mau> <size> <rom|ram>`, `map <space id> <start in space> <size in space> <chip id> <start on chip>`, `#` comments. It gives every region's exact start and size. It is not MDF4.

## Decisions

Final once the owner answers Q1–Q3. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Three shippable parts.** Part A (tables and fixes) and Part B (address-space model) run in parallel. Part C (Rich layout) needs both. Each part ends in `cargo xtask gate`, Windows CI, an Opus high review, merge and an alpha build. The plan has 33 atoms; the orchestrator may save the parts as `p38a-tables`, `p38b-space-model` and `p38c-rich-layout`, with these decisions shared.
- **D2 — Layout: direction 1, "Address space first".** From the prototype of 2026-10-20. Top to bottom:
  1. **Address space** panel. A header row with the title, the legend (code, rodata, data, bss, other, free) and a two-way switch "Compress gaps / True scale". Then one row per region: on the left the region name, the used percentage in large figures and a line "used · free · of total" (plus "reserved" for a MAP that gives it); on the right the used bar and, under it, the memory strip with its address axis. The flash region's strip is 40 px high, the others 28 px.
  2. **File bytes** (ELF only). One bar with loaded bytes, DWARF, notes, symbols and strings, and other, each with its size and share.
  3. **Body in two columns (5 : 7).** Left: "Who uses the flash", a treemap with the grouping switch. Right: the table (Symbols for an ELF, Located sections for a MAP), largest first, with the noise bar above it.
  - Under 1,100 px of content width the body columns stack, and region rows put the bars under the name.
  - Not adopted from the prototype: directions 2 and 3, the lens, the ranked list, the "Top 50 / Show all" button (the table is virtualised and shows every row), and docking the drawers at 1,680 px.
- **D3 — Simple stays as today.** The Rich layout appears only when the preset is Rich, or Custom with at least one of Memory strip, Used and free bars or Who uses the flash on. Simple keeps today's table, navigator, inspector drawer and status line. The table changes of D11–D13 are table behaviour, not optional parts, so they apply in every mode (see Q3).
- **D4 — Drawers stay over the content on every width (P16 D11, D58).** Ctrl+B navigator and Ctrl+I inspector are unchanged. The inspector shows the **full name** of the selection, never truncated: a strip block, a treemap tile, a group or a table row. The status line shows the same full name while something is selected.
- **D5 — Custom parts (D58).** A new part set `AUTOMOTIVE_LAYOUT` for ELF and MAP:

  | Part | Setting | Off means |
  | --- | --- | --- |
  | Open the inspector | `automotive.view.inspector` (exists) | the inspector opens only on Ctrl+I |
  | Memory strip | `automotive.view.memory-strip` (exists) | no strips and no axis lines |
  | Used and free bars | `automotive.view.usage-bars` (new) | no region percentages, used bars or File bytes bar |
  | Who uses the flash | `automotive.view.who-uses` (new) | no treemap; the table takes the full width |
  | Row bars | `automotive.view.row-bars` (exists) | no Share column in the table |

  - `charts` stays for the CAN trace and later views; ELF and MAP do not list it.
  - `family` is all six automotive fields, so Custom copies every one of them. HEX/MOT keep `AUTOMOTIVE`.
  - Each new setting gets one sentence in the settings sheet, in `settings.ftl`.
- **D6 — Rich never assumes one toolchain's layout (owner: "ELF files will look different between them").** It builds from facts that every ELF has and takes each piece from the best source available. A **provenance line** under the Rich header says which source it used, for example "regions from demo.mdf · owners from symbols (12 estimated) · kinds from section flags". The line shows in Rich and Custom only.
- **D7 — Kinds.**
  - ELF: from the section type and flags, never from names. Alloc and exec → code; alloc and NOBITS → bss; alloc and write → data; alloc only → rodata; anything else → other. ABS symbols are other. One exception, taken from real TASKING files: a writable NOBITS section named `.data`, `.zdata` or `.sdata` (or `.data.*` and so on) is data, not bss.
  - **Fill** (added 2026-10-20 after the owner's demo.elf): linker fill is its own kind. An ELF section whose name is `$fill` or starts with `$fill` (TASKING), and a MAP entry named `*fill*` or `$fill` (GNU ld, TASKING), is Fill whatever its flags. Fill bytes are in the image but are not "used": `Region.used` leaves them out, `by_kind` counts them, and owners never claim them. The bars show Fill as a hatched part of free space, labelled "Linker fill"; the strip gives it its own token; the treemap leaves it out.
  - MAP alone has no flags. A located section takes the kind of the sibling ELF section with the same name and address when one is loaded. Otherwise the kind comes from the section name's leading part (`.text`, `.rodata`, `.data`, `.bss` and their small-data forms such as `.zdata`, `.zbss`, `.zrodata`); a bracketed TASKING ROM copy `[.data.X]` is rodata. The provenance line then says "kinds from section names".
- **D8 — Region totals and free space (scope item 8).** An ELF knows what it uses, not what each region holds. Sources, best first:
  1. **`<stem>.mdf`** beside the file: every chip mapped into the address space becomes a region with its exact start, size and type (rom or ram).
  2. **`<stem>.map`** beside the file: region names, totals, used and free from its memory table. The start comes from the map when it gives one, else from the `.mdf`, else from the lowest located section address in that region ("start estimated").
  3. **Address clusters**: the ELF's alloc sections, sorted by address, split where the gap to the next one is at least 16 MiB. A cluster shows **used only**. Its row says "free not known: no .map or .mdf beside this file"; there is no free figure and no free gaps in its strip.
  - Lookup: the same folder, the same stem, extension `.mdf` or `.MDF`, then `.map` or `.MAP`. When no file has the same stem and the folder holds exactly one `.mdf` (or one `.map`), that file is used and the provenance line says so ("regions from build.mdf, the only .mdf here"). A missing or unreadable sibling counts as absent. A `.map` that the TASKING sniffer rejects counts as absent. Owner, Q2: the build writes the `.elf`, `.hex`, `.mot`, `.map` and `.mdf` into the same folder, usually with the same stem.
  - The siblings load in the background after the file's first content. Simple never waits for them; Rich redraws when they arrive. Limits: 1 MiB for a `.mdf`, 64 MiB for a `.map`.
  - A MAP view uses the same rule for its region starts: the map's own origin, else `<stem>.mdf`, else the lowest section address.
  - A region name from the `.map` matches a `.mdf` chip name case-insensitively.
  - **Trade-off.** A sibling can be stale or come from another build, and the app cannot tell. So the source is named on screen, and ELF sections that fall outside every region from a sibling are a problem in the status line ("N sections outside the regions from demo.mdf"), shown in every mode. Rejected: asking the user to pick a file on open (it breaks the calm open), and built-in chip tables per CPU (they go stale and miss derivatives).
- **D9 — Owner of each byte (ELF), in this order.**
  1. A sibling MAP gives exact object files for each located section, matched by section name and address.
  2. Per-function sections (more sections than one per kind in a region) own their bytes by section name.
  3. Sized symbols own `[value, value + size)`.
  4. A zero-size GLOBAL or WEAK FUNC or OBJECT symbol gets a size estimated up to the next symbol address in the same section, and is marked "estimated". LOCAL zero-size symbols (compiler labels) never get an estimate, so labels never split a function.
  5. Bytes of a loaded section that no owner covers go to "(not attributed)" for that section, so the treemap always adds up to the region's used bytes.
  - FILE symbols name an object only when they end in `.c`, `.cpp`, `.cc`, `.s`, `.S`, `.asm`, `.o` or `.obj`; compiler temp files are ignored.
  - A stripped ELF (no symbol table) has sections and segments only. "Who uses the flash" shows a note instead of the treemap: "This file has no symbol table, so bytes cannot be traced to modules."
- **D10 — Grouping.** A switch with Module, Source file, Object file and Section. The default is the first one in that order that yields more than one group. An option without data is disabled and its tooltip says why.
  - **Module:** from the owner's name (object file stem, else section name without its leading kind part, else symbol name), without leading `.` or `_`. Rules, first match wins (owner, Q1: "the best way to cover everything"):
    1. the user's list `[automotive] modules = ["Com", "Dio_Cfg"]` in `navo.toml`: the longest listed prefix that the name starts with, followed by `_`, `.` or the end of the name;
    2. AUTOSAR two-part prefixes: `Rte_<X>` and `SchM_<X>` group as `Rte_<X>` and `SchM_<X>`;
    3. the text up to the first `_` or `.`.
    - Grouping ignores case and shows the first spelling met. Names that leave nothing go to "(other)". The list reloads live like other settings (P22), and the Rich header's provenance line says "modules: N from your list" when the list matched anything.
  - **Source file:** the DWARF source file of the owner's first address; else a FILE symbol with a real extension (D9).
  - **Object file:** from a MAP, or FILE symbols ending in `.o` or `.obj`.
  - **Section:** the output section name.
  - **The treemap colours by group** (amended; owner, 2026-10-21): one generated hue per top-level group, children take shades of it; kind shows in the thin kind bar. Hues step by the golden angle from a fixed start, largest group first; part shades alternate light and dark inside a narrow band.
  - "Who uses the flash" uses the region with the most code and rodata bytes (a rom chip first when a `.mdf` gives types). The hint names the region.
  - Treemap: area is bytes in that region. The top 22 groups get tiles, the rest one "N more" tile. A tile larger than 150 × 110 px shows up to 14 inner tiles (object files in a module for a MAP, owners otherwise). Tile text shows when the tile is larger than 64 × 34 px; the full name is always in the tooltip, the inspector and the accessibility label. A 4 px kind bar sits at the bottom of each tile.
  - Selecting a group dims every strip block outside it and shows the group in the inspector: bytes per kind, item count and the eight largest items. Esc clears the selection. The table is not filtered.
- **D11 — Shared resizable table columns (component boundary).** A new primitive `primitives::columns`, used by the ELF, MAP, CAN trace, CSV and `.mdf` tables.
  - **It owns:** the column specs (id, title, alignment, min and max width, one flexible column), the widths in pixels, the initial widths from content, the header row with sort-arrow slot and resize handles, live drag resizing, and double-click on a handle to fit that column.
  - **It gives the viewer:** `width(column) -> Pixels` for row cells, and the events `Sort(column)` and `Resized`.
  - **The viewer keeps:** row rendering, the virtual list, sorting, filtering, keys and the CSV view's horizontal column paging.
  - **Initial widths:** header text plus the widest cell in the first 200 rows, clamped to the spec. The flexible column (the name) takes what is left and never shrinks below its minimum. The character width is measured once from the mono font; 7.2 px is only the fallback.
  - Widths live with the open file and are not saved. Reset in the View menu also resets them.
  - It replaces the four copies of `CHAR_WIDTH` and `chars()`. The CAN trace moves from one formatted string per row to cells: Time, Ch, ID, Dir, Type, DLC, Data (flexible).
- **D12 — Long names keep their prefix.** `fit_name(prefix, rest, chars) -> Fitted { prefix, rest }`. A name that fits stays whole. Otherwise the prefix keeps at most 30 % of the width (shortened to "ab…." when longer). The rest is cut in the middle with 45 % of it before the ellipsis. Minimum 6 characters. The prefix is drawn muted.
  - Prefix rules: MAP section names of the form `.kind.<module>.<rest>` → `<module>.`; symbols → the text up to the first `_` when it falls within characters 2–16; else no prefix.
  - The full name is always in the inspector, the tooltip, the status line and the accessibility label.
- **D13 — ELF "Hide zero-size and ABS" (scope item 4).** `Query.hide_noise`, on by default for every file. Hidden: size 0 with no estimate (D9), ABS place, and FILE or SECTION symbols.
  - Rich and Custom show a noise bar above the table: a checkbox "Hide zero-size and ABS", "56,910 of 59,494 symbols hidden (labels, file names, absolute)" and "2,538 shown" on the right.
  - Every mode shows "N hidden (zero-size, ABS)" in the status line.
  - Alt+H toggles it for the open file.
  - Ctrl+G still finds a hidden symbol. The note then says "Hidden by the zero-size filter; Alt+H shows it."
- **D14 — MAP fixes (scope item 6).** The causes are measured, so no checker atom is needed; the fix atom writes the failing tests first.
  - **Part headers.** A line is a part header only when it is one of these shapes:
    - a run of at least three `*`, a title and at least three `*`;
    - a `* Title` line followed by a line of `=` only (TASKING's sub-headers);
    - a run of at least three `=`, a title and at least three `=` (kept for the tolerance fixtures);
    - a bare `* Title` line whose title is a known TASKING part title (the titles the parser reads, plus the ones it skips, matched case-insensitively), kept for the tolerance fixtures.
    - Call-graph lines (`* name [n,m]`) and any other `* text` line are not headers. Amended 2026-10-20 after A9 found the tolerance fixtures.
  - **Unread parts.** `MapModel.skipped_parts: Vec<String>` replaces `skipped_sections`: titles in file order, without duplicates, at most 32. The status line says "Not read: <titles>" with the first three titles and "+N more"; the navigator lists them all under "Not read".
  - **Space.** Within one Symbols table, a blank Space cell repeats the space of the row above. The table's first rows start with the `+ Space` heading when one exists.
  - **Size.** TASKING Symbols tables have no size. The Size column is hidden when no row in the table has a size, and the status line says "This map lists no symbol sizes." Sizes from a sibling ELF come later, not in P38.
- **D15 — HEX/MOT (scope item 5).**
  - **Folding.** A gap shorter than 256 bytes folds into the range around it: its bytes show `--` inside the rows, with no gap row. The navigator lists the folded range once with "N padding gaps". The status line keeps the gap count and adds "N folded".
  - **Width.** 32 bytes per row when the content is wide enough for a 32-byte row with the measured mono width, else 16. The choice is made again on resize, and the cursor keeps its address.
  - `Layout::new(ranges, LayoutOptions { row_bytes, fold_under })` replaces the fixed `ROW_BYTES`.
- **D16 — `.mdf` viewer (scope item 7).**
  - **Sniffing:** extension `.mdf`, and the first non-comment lines start with `space`, `chip` or `map`. An MDF4 file starts with the binary `MDF     ` magic, so it never matches.
  - **Layout:** one table of chips (Chip, Type, Size, Space, Start in space, End, Start on chip). The spaces are listed in the header line, and problems (unknown ids, overlaps, bad numbers) in the status line.
  - It uses the D11 columns and the `AUTOMOTIVE_TABLE` parts (inspector only).
- **D17 — Memory strip.**
  - **Blocks:** owners at section level (sections for an ELF, located sections for a MAP), in address order.
  - **Gap compression** (default): a free run longer than 4 % of the region is drawn as 1.5 % with a break mark. The axis says "N free gaps compressed (size)". Alt+Z or the switch selects True scale, for the open file.
  - **Merging:** blocks under one pixel merge into one cell per pixel column, coloured by the kind with the most bytes. Its tooltip says "N sections · size".
  - **Merged layouts:** a block wider than 64 px that holds sized symbols draws them as inner cells with thin dividers. Enter on a selected block zooms the strip to it, and Backspace zooms out when the strip has focus and is zoomed. This keeps the strip useful when one section fills most of the flash.
  - **Hover tooltip:** the full name, kind, size, address, and the object file when known. A caret marks the selected block.
  - **Cost:** cells are computed off the render path. They are cached per region, width (8 px steps), scale mode and zoom, and recomputed only when one of these changes.
- **D18 — Keys (ELF and MAP).** Checked against `keymap.toml` at 9636e33: none of these is bound in the Elf or Map context, and letters still type into the filter.

  | Key | Does |
  | --- | --- |
  | Tab / Shift+Tab | Moves focus: table → each region's strip → treemap → table |
  | ← / → (strip) | Previous or next block, with the inspector following |
  | ↑ / ↓ (strip) | The strip of the region above or below |
  | Home / End (strip) | First or last block |
  | Enter (strip) | Zooms into the selected block (D17) |
  | Backspace (zoomed strip) | Zooms out; elsewhere it edits the filter as today |
  | Arrows, Enter (treemap) | Move between tiles in reading order; select a group |
  | Alt+G | Next grouping (D10) |
  | Alt+Z | Compress gaps / True scale |
  | Alt+H | Hide zero-size and ABS on or off (ELF) |

  The keys show in the F-key bar hints and the View menu (D58 rule 7). They get `cmd-` labels in `commands.ftl`.
- **D19 — Colours.** New tokens `mem_code`, `mem_rodata`, `mem_data`, `mem_bss`, `mem_other`, each with a strong, a tint and a text step, plus `mem_free` and `mem_free_line`, in both themes.
  - Hues follow the prototype and D64: code 40, rodata 82, data 210, bss 238, other neutral 60 at low chroma. They are generated with `palette.mjs`.
  - Contrast: strong steps reach 3:1 against the background (UI parts); tint and chip text reach 4.5:1; tile text reaches 4.5:1 on its tint. The checks live in `tokens/contrast.rs`.
  - Colour never carries meaning alone: the legend, chips, tooltips and the inspector name the kind.
  - Motion: only the existing 150 ms fade, off with reduce-motion. Nothing flashes.
- **D20 — Scale (scope item 9).**
  - **Fixtures:** generated ELF files with invented names: a ~30 MB "merged" file (about 120 sections, 50 LOAD segments, 210,000 symbols, 95 % zero-size LOCAL labels, DWARF) and a "split" file (3,500 sections, one per function, 3,400 LOAD segments, 210,000 symbols). Plus small fixtures: stripped, 32 and 64 bit, little and big endian, with and without DWARF, and with and without a sibling `.map` and `.mdf`.
  - **Budgets** (release build, one run on the dev PC, recorded in the Log): parse plus DWARF lines ≤ 300 ms; the Rich model (regions, owners, groups) ≤ 150 ms; strip cells for 1,600 px ≤ 5 ms per region; treemap layout ≤ 1 ms; `select` with `hide_noise` ≤ 50 ms; peak memory ≤ 200 MB (P16 D10). The first Rich frame after the index is ready ≤ 100 ms in the headless window.
  - The budget tests are `#[ignore]`d release tests run by the atom check. They are not CI gates: D63 adds no new performance gates before the alpha features are done.
  - Table rows stay virtualised (`VirtualList`). The model is built in the background job, never in `render`.
- **D21 — Text.** All new UI text goes into `viewer.ftl`, `settings.ftl` and `commands.ftl` (D59). The `l10n` gate step must pass.
- **D22 — Who builds what (the owner's ladder).**
  - Sonnet medium (`ui-builder`): decided UI.
  - Sol 6.1 xhigh (`api-builder` through crew): backend parsing and model work, never UI.
  - Opus medium (`ui-builder` or `api-builder`): senior UI, custom drawing, cross-cutting interaction.
  - Opus high (`reviewer`): the reviews.
  - Every check runs with `nice -n 10` and `CARGO_BUILD_JOBS=8`, one gate at a time.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `mdf` parser | automotive lib | `parse_mdf(&str) → Mdf { spaces, chips, maps, problems }`, `sniff_mdf(head) → bool`, `Mdf::regions() → RegionTable` | new `src/mdf/` | `tests/mdf.rs` |
| `space::kind` and regions | automotive lib | `Kind::of_section(&Section)`, `Kind::of_name(&str)`, `RegionTable`, `regions_for_elf(&Elf, Option<&RegionTable>) → Space`, clusters | new `src/space/` | `tests/space_regions.rs` |
| `space::owners_elf` | automotive lib | `owners(&Elf, Option<&Lines>, Option<&MapModel>) → Owners` with provenance and estimate counts | new | `tests/space_owners.rs` |
| `space::owners_map` and groups | automotive lib | `owners_of_map(&MapModel) → Owners`; `group(&Owners, Grouping, region) → Vec<Group>`; `default_grouping` | new | `tests/space_groups.rs` |
| `space::strip` | automotive lib | `Scale::new(region, ScaleMode, Option<Range>)`, `cells(region, &Scale, width_px) → Vec<Cell>`, breaks, inner cells | new | `tests/space_strip.rs` (unit + proptest) |
| `space::treemap` | automotive lib | `squarify(&[Item], Rect) → Vec<Tile>`, `top(groups, 22) → Vec<Item>` | new | `tests/space_treemap.rs` |
| ELF noise filter | automotive lib | `Query.hide_noise`, `select → Selection { rows, hidden }` | change `src/elf/query.rs` | `tests/elf_query.rs` |
| MAP headers, Space, sizes | automotive lib | `skipped_parts`, ditto space, `has_symbol_sizes()` | change `src/map/` | `tests/map.rs`, `tests/map_tolerance.rs` |
| `Layout` options | automotive lib | `Layout::new(ranges, LayoutOptions)`, `folded()` | change `src/layout.rs` | `tests/layout.rs` |
| large fixtures and budget | automotive tests | `elf_build` layouts; `tests/elf_rich_large.rs` | new | itself |
| `fit_name` | ui lib | `fit_name(prefix, rest, chars) → Fitted` | new `primitives/columns/fit.rs` | unit tests |
| `columns` | ui primitive | `ColumnSet` entity, `ColumnHead` element, `width(col)`, events `Sort`, `Resized` | new `primitives/columns/` | `primitives/columns/tests.rs`, gallery |
| ELF, MAP, CSV, trace, `.mdf` tables | ui views | use `columns` | change | each view's tests |
| memory-kind tokens | ui tokens | `colors.mem_*` | change `tokens/` | `tokens/contrast.rs` tests |
| `AUTOMOTIVE_LAYOUT` parts | ui + settings | two new fields, actions, menu rows | change | `parts/tests.rs`, `settings/tests.rs` |
| `space::sibling` | ui adapter | `load_siblings(hooks, location) → Siblings { mdf, map, provenance }` in a background job | new `viewer/space/sibling.rs` | `viewer/space/tests.rs` (fake hooks) |
| bars, legend, file bar | ui presentational | `UsageBar`, `Legend`, `FileBytesBar`, `RegionRow` props | new `viewer/space/bars.rs` | logic tests + gallery |
| `MemoryStrip` | ui presentational | cells, breaks, caret, dim set in; `Hover(block)`, `Pick(block)` out | new `viewer/space/strip.rs` | tests + gallery |
| `Treemap` | ui presentational | tiles in; `Pick(group)` out | new `viewer/space/treemap.rs` | tests + gallery |
| Rich container | ui container | `RichSpace` state per file (scale, zoom, grouping, focus part), layout by parts | new `viewer/space/mod.rs`, `layout.rs` | `viewer/space/tests.rs` |
| strip and focus keys | ui | D18 | new `viewer/space/keys.rs` | `viewer/space/tests.rs` |
| HEX/MOT view | ui view | 32-byte rows, folded gaps | change `viewer/memory/` | `memory/tests.rs` |
| `.mdf` view | ui view | table kind | new `viewer/mdf/` | `viewer/mdf/tests.rs` |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `fit_name` | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-ui columns::fit` | done | |
| A2 | `columns` primitive | ui-builder (Opus) | medium | A1 | `cargo nextest run -p navo-ui primitives::columns`; gallery shot | done | |
| A3 | ELF table on `columns` | ui-builder (Sonnet) | medium | A2 | `cargo nextest run -p navo-ui viewer::elf` | done | |
| A4 | MAP table on `columns` | ui-builder (Sonnet) | medium | A2 | `cargo nextest run -p navo-ui viewer::map` | done | |
| A5 | CSV table on `columns` | ui-builder (Sonnet) | medium | A2 | `cargo nextest run -p navo-ui viewer::table` | moved to beta (D74/D75) | orchestrator 2026-10-26 |
| A6 | CAN trace rows as cells | ui-builder (Sonnet) | medium | A2 | `cargo nextest run -p navo-ui viewer::trace viewer::tests::can_trace` | moved to beta (D74/D75) | orchestrator 2026-10-26 |
| A7 | ELF noise filter (model) | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-automotive elf_query` | done | |
| A8 | ELF noise filter (UI) | ui-builder (Sonnet) | medium | A7, A3 | `cargo nextest run -p navo-ui viewer::elf`; `cargo xtask gate` (l10n) | done | |
| A9 | MAP headers, Space, sizes (model) | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-automotive map` | done | |
| A10 | MAP status and Size column (UI) | ui-builder (Sonnet) | medium | A9, A4 | `cargo nextest run -p navo-ui viewer::map` | done | |
| A11 | `Layout` options (model) | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-automotive layout memory` | done | |
| A12 | HEX/MOT 32-byte rows and folded gaps (UI) | ui-builder (Sonnet) | medium | A11 | `cargo nextest run -p navo-ui viewer::memory` | done | |
| A13 | review Part A | reviewer (Opus) | high | A3–A6, A8, A10, A12 | review notes; gate; Windows CI | done: one Opus review for A13 and C11; fixes in the review fix round | |
| B1 | `mdf` parser | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-automotive mdf` | done | |
| B2 | `space::kind` and regions | api-builder (Sol 6.1) | xhigh | B1 | `cargo nextest run -p navo-automotive space_regions` | done | |
| B3 | `space::owners_elf` | api-builder (Sol 6.1) | xhigh | B2 | `cargo nextest run -p navo-automotive space_owners` | done | |
| B2b | Fill kind (D7) in kinds, regions and owners | api-builder (Sonnet) | high | B3 | `cargo nextest run -p navo-automotive space_regions space_owners` | done | |
| B4 | `space::owners_map` and groups | api-builder (Sonnet) | high | B3, B2b | `cargo nextest run -p navo-automotive space_groups` | done | |
| B5 | `space::strip` | api-builder (Sol 6.1) | xhigh | B2 | `cargo nextest run -p navo-automotive space_strip` | done | |
| B5b | `strip::inner` (D17 inner cells from owners) | api-builder (Sonnet) | medium | B5, B3 | `cargo nextest run -p navo-automotive space_strip` | done | |
| B6 | `space::treemap` | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-automotive space_treemap` | done | |
| B7 | fixtures and budget | api-builder (Sol 6.1) | xhigh | B4, B5, B6, A7 | `cargo nextest run --release -p navo-automotive --test elf_rich_large --run-ignored all` | done | |
| B8 | `.mdf` viewer | ui-builder (Sonnet) | medium | B1, A2 | `cargo nextest run -p navo-ui viewer::mdf`; gallery shot | done | |
| B9 | review Part B | reviewer (Opus) | high | B7, B8 | review notes; gate; Windows CI | done | |
| C1 | memory-kind tokens | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-ui tokens` | done | |
| C2 | `AUTOMOTIVE_LAYOUT` parts and settings | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-ui parts settings viewer::tests::view_parts`; `cargo xtask gate` (l10n) | done | |
| C3 | `space::sibling` loader | ui-builder (Opus) | medium | B2, B4 | `cargo nextest run -p navo-ui viewer::space::sibling` | done | |
| C4 | bars, legend, File bytes | ui-builder (Sonnet) | medium | C1, B2 | `cargo nextest run -p navo-ui viewer::space::bars`; gallery shots | done | |
| C5 | `MemoryStrip` element | ui-builder (Opus) | medium | C1, B5 | `cargo nextest run -p navo-ui viewer::space::strip`; gallery shots | done | |
| C6 | `Treemap` element | ui-builder (Sonnet) | medium | C1, B6 | `cargo nextest run -p navo-ui viewer::space::treemap`; gallery shots | done | |
| C7 | Rich container in the ELF view | ui-builder (Opus) | medium | C2–C6, A8, B9 | `cargo nextest run -p navo-ui viewer::space viewer::elf`; screenshots | done | |
| C8 | Rich container in the MAP view | ui-builder (Sonnet) | medium | C7, A10 | `cargo nextest run -p navo-ui viewer::space viewer::map`; screenshots | done | |
| C9 | strip, treemap and Alt keys | ui-builder (Opus) | medium | C8 | `cargo nextest run -p navo-ui viewer::space::keys commands`; `cargo xtask gate` | done | |
| C10 | large-file UI check | ui-builder (Opus) | medium | C9, B7 | `cargo nextest run --release -p navo-ui viewer::space::large --run-ignored all` | done | |
| C11 | review Part C | reviewer (Opus) | high | C10 | review notes; gate; Windows CI; owner check list | done: one Opus review for A13 and C11; fixes in the review fix round | |

Parallel lanes:
- A1 → A2 → A3, A4, A5, A6 in parallel. A7, A9 and A11 start at once.
- B1 → B2 → B3 → B4, with B5 after B2; B6 starts at once.
- C1 and C2 start at once. C3–C6 wait for their B atoms. C7 waits for Parts A and B.

### A1 — `fit_name`

- **Read:** D12; `crates/navo-ui/src/primitives/mod.rs`; `crates/navo-ui/src/format.rs` (style).
- **Change:** new `primitives/columns/mod.rs` (module stub that only declares `fit`) and `primitives/columns/fit.rs`:
  - `pub struct Fitted { pub prefix: String, pub rest: String }`;
  - `pub fn fit_name(prefix: &str, rest: &str, chars: usize) -> Fitted` following D12, counting `char`s, never splitting a UTF-8 code point;
  - `pub fn map_prefix(name: &str) -> (&str, &str)` and `pub fn symbol_prefix(name: &str) -> (&str, &str)` for the two prefix rules.
- **Check:** `cargo nextest run -p navo-ui columns::fit`. Tests: fits unchanged; long rest cut in the middle at 45 %; long prefix shortened to `ab….`; minimum 6; non-ASCII names; a proptest that the result never exceeds `max(chars, 6)` and keeps the first and last character of `rest`.
- **Stop if:** `primitives` has a module-size or export rule that forbids a nested folder.

### A2 — `columns` primitive

- **Read:** D11; `primitives/splitter.rs` (drag handling, `DragMoveEvent`); `viewer/table/draw.rs:100-170` (head cells, content widths); `viewer/elf/draw.rs:16-60`; `navo-structured/src/table/mod.rs:105-125` (`column_widths`).
- **Change:** in `primitives/columns/`:
  - `ColumnSpec { id, title, align, min_chars, max_chars, flex: bool }`.
  - `ColumnSet` with `new(specs)`, `fit_content(header, rows: impl Iterator<Item = Vec<String>>)` (first 200 rows), `width(at) -> Pixels`, `set_width(at, Pixels)`, `fit(at)`, `reset()`. The character width is measured once from the mono font through the window's text system; 7.2 px is the fallback.
  - `ColumnHead` element: header cells, a 6 px handle at each right edge with the resize cursor, live drag, double-click fits. It emits `ColumnEvent::Sort(at)` on a title click and `ColumnEvent::Resized`.
  - A gallery page `columns` with a five-column table.
- **Check:** `cargo nextest run -p navo-ui primitives::columns`. Tests: content widths clamp to the spec; the flex column takes the rest and keeps its minimum; a drag of +40 px widens one column; a double-click fits; reset restores the initial widths; a title click emits `Sort`. Gallery shot in both themes.
- **Stop if:** GPUI cannot report a drag on a child of a header cell without starting a parent click; report it and do not add timers.

### A3 — ELF table on `columns`

- **Read:** `viewer/elf/draw.rs` (whole), `viewer/elf/mod.rs:79-140`, A2's `columns` API.
- **Change:** remove `CHAR_WIDTH`, `chars()` and fixed widths. `ElfView` holds `Entity<ColumnSet>` built after the index from the first 200 rows. Name cells use `fit_name` with `symbol_prefix`; the accessibility label keeps the full name. Header clicks keep today's sort keys.
- **Check:** `cargo nextest run -p navo-ui viewer::elf`. New tests: widths follow content; a resize changes the row cell width; a long name keeps its prefix and the label keeps the full name; Ctrl+F3 sorting still works.
- **Stop if:** `elf/draw.rs` needs more than 150 changed lines; split the head into `elf/head.rs` first and report.

### A4 — MAP table on `columns`

- **Read:** `viewer/map/draw.rs` (whole), `viewer/map/select.rs:30-110`, A2's API.
- **Change:** as A3, with one `ColumnSet` per table (Memory, Sections, Symbols) kept while the file is open. Section names use `map_prefix`; symbols use `symbol_prefix`.
- **Check:** `cargo nextest run -p navo-ui viewer::map`. Tests: each table keeps its own widths across Alt+T; long names keep their prefix; sort keys unchanged.
- **Stop if:** `select.rs` heads carry widths that other code reads; report the readers.

### A5 — CSV table on `columns`

- **Read:** `viewer/table/draw.rs`, `viewer/table/mod.rs`, `viewer/table/jobs.rs:120-140`, A2's API.
- **Change:** the CSV view uses `ColumnSet` widths seeded from `column_widths`. It keeps its own horizontal paging (`left`, `visible_columns`, `shows_column`), now on pixel widths.
- **Check:** `cargo nextest run -p navo-ui viewer::table`. Tests: a resize changes paging; a wide file still pages; existing tests stay green.
- **Stop if:** paging needs the column set to own scrolling; report it (D11 keeps paging in the view).

### A6 — CAN trace rows as cells

- **Read:** `viewer/trace/text.rs:1-60` and `130-218`, `viewer/trace/draw.rs:60-140`, A2's API.
- **Change:** `row()` returns `Vec<String>` cells for Time, Ch, ID, Dir, Type, DLC, Data plus the tone; `column_titles()` becomes the specs. Data is the flex column. Error rows keep their tone and left border. `trace_label` stays one string for accessibility.
- **Check:** `cargo nextest run -p navo-ui viewer::trace viewer::tests::can_trace`. Tests: cells for a frame, an error frame and an "other" line; resize; follow mode still keeps the last row in view.
- **Stop if:** an "other" line (comments, events) cannot fit the cell model; report its shape.

### A7 — ELF noise filter (model)

- **Read:** D9 rule 4, D13; `navo-automotive/src/elf/query.rs` (whole), `src/elf/mod.rs:85-130`, `tests/elf_query.rs`.
- **Change:** `Query.hide_noise: bool` (default `true`). `select` returns `Selection { rows: Vec<u32>, hidden: u64 }`. Hidden means: size 0 and no estimate, ABS place, or FILE or SECTION kind. Add `pub fn estimated_size(elf, at) -> Option<u64>` for D9 rule 4 (GLOBAL or WEAK FUNC or OBJECT, up to the next symbol address in the same section). Update callers inside the crate only.
- **Check:** `cargo nextest run -p navo-automotive elf_query`. Tests: counts on a fixture with labels, ABS, FILE and sized symbols; an estimated global is shown; a LOCAL zero-size label is hidden; `hide_noise = false` shows every symbol.
- **Stop if:** the UI crate's callers break in a way that needs more than renaming `rows` to `selection.rows`; list them for A8.

### A8 — ELF noise filter (UI)

- **Read:** D13; `viewer/elf/jobs.rs:20-137`, `viewer/elf/text.rs`, `viewer/elf/go_to.rs`, `viewer/elf/keys.rs:1-60`, `commands/keymap.toml` (Elf context), `navo-l10n/locales/en-US/viewer.ftl`, `commands.ftl`.
- **Change:** the hidden count goes into the status line in every mode. Alt+H toggles `hide_noise` for this file through a new `elf_view::ToggleNoise` action, `elf.toggle_noise` command, `cmd-` label and key. Ctrl+G on a hidden symbol sets the D13 note. The Rich noise bar comes in C7.
- **Check:** `cargo nextest run -p navo-ui viewer::elf`, then `cargo xtask gate`. Tests: status shows the count; Alt+H shows all; Ctrl+G to a hidden label gives the note.
- **Stop if:** Alt+H is taken in a parent context that the Elf context inherits.

### A9 — MAP headers, Space, sizes (model)

- **Read:** D14; `navo-automotive/src/map/parts.rs`, `map/tasking.rs:1-120`, `map/rows.rs:44-60`, `map/model.rs`, `tests/map.rs`, `tests/map_tolerance.rs`.
- **Change:**
  - `parts::header` accepts only the D14 header shapes. A `* Title` line counts only when the next line is `=` only, so `Tasking::take` keeps one line of look-behind for the title.
  - `skipped_sections: u64` becomes `skipped_parts: Vec<String>` (titles, deduped, at most 32).
  - Ditto space inside one Symbols table.
  - `MapModel::has_symbol_sizes() -> bool`.
- **Check:** `cargo nextest run -p navo-automotive map`. Red first, with invented fixtures: 50 call-graph lines give zero unread parts; three titled parts give three names; a Symbols table with one Space cell gives that space on every row; a table without a size column gives `has_symbol_sizes() == false`. The `map_wrap` fixtures stay green.
- **Stop if:** a TASKING fixture in the tests uses a header shape that D14 does not list; report it.

### A10 — MAP status and Size column (UI)

- **Read:** D14; `viewer/map/text.rs:180-210`, `viewer/map/select.rs:35-110`, `viewer/map/nav.rs`, `viewer.ftl`.
- **Change:** status "Not read: <first three titles> +N more" and "This map lists no symbol sizes"; the navigator gets a "Not read" group with every title; the Symbols table drops Size when `has_symbol_sizes()` is false.
- **Check:** `cargo nextest run -p navo-ui viewer::map`. Tests for each text and for the hidden column.
- **Stop if:** the navigator's item enum cannot hold a non-selectable row; report.

### A11 — `Layout` options (model)

- **Read:** D15; `navo-automotive/src/layout.rs`, `src/image.rs:55-120`, `tests/layout.rs`.
- **Change:** `LayoutOptions { row_bytes: u64 (16 or 32), fold_under: u64 (256) }`; `Layout::new(&ranges, options)`; `Row::Data` gains `missing: Vec<Range<u64>>` or an equivalent that marks folded bytes; `Layout::folded() -> u64`; `row_bytes()`. Keep `ROW_BYTES` only as the default.
- **Check:** `cargo nextest run -p navo-automotive layout`. Tests: 198 ranges with 16-byte gaps fold to 2 blocks; a 300-byte gap stays a gap row; 32-byte rows; `row_of` and `windows` agree for both widths (proptest).
- **Stop if:** a folded gap overlapping a row start breaks `windows`; report the case.

### A12 — HEX/MOT 32-byte rows and folded gaps (UI)

- **Read:** D15; `viewer/memory/mod.rs:1-215`, `memory/jobs.rs:55-70`, `memory/draw.rs:1-160`, `memory/text.rs`, `memory/drawers.rs` (navigator rows).
- **Change:** pick `row_bytes` from the content width (measured mono width) and rebuild the layout on change, keeping the cursor address. The column header is built from `row_bytes`. Folded bytes draw `--` in the faint colour. Navigator: "N padding gaps" on folded ranges. Status: "N folded".
- **Check:** `cargo nextest run -p navo-ui viewer::memory`. Tests: wide window → 32 per row; narrow → 16; resize keeps the address; folded bytes render `--`.
- **Stop if:** the layout must be rebuilt in `render`; move it to the resize observer or report.

### A13 — review Part A

Opus high on A1–A12: column widths against content, prefix truncation on non-ASCII names, the noise filter's hidden count, the MAP header rule against the `map_wrap` fixtures, and the folded layout's `row_of` and `windows`. Then gate, Windows CI, merge and an alpha build.

### B1 — `mdf` parser

- **Read:** D16 and the `.mdf` facts above; `navo-automotive/src/lib.rs`, `src/map/mod.rs` (module style), `src/map/model.rs` (problem style).
- **Change:** `src/mdf/mod.rs` and `parse.rs`:
  - `Mdf { spaces: Vec<Space { name, id, mau }>, chips: Vec<Chip { name, id, mau, size, kind: ChipKind }>, maps: Vec<MapLine { space, start, size, chip, chip_start }>, problems: Vec<MdfProblem> }`.
  - `parse_mdf(&str) -> Mdf`: tabs or spaces, decimal or `0x` numbers, `#` comments, at most 10,000 lines.
  - `sniff_mdf(head: &[u8]) -> bool`.
  - `Mdf::regions() -> RegionTable` (`RegionTable` defined here, reused by B2): name, start, size, rom or ram, per mapped chip.
- **Check:** `cargo nextest run -p navo-automotive mdf`. Invented fixtures: three chips mapped; an unknown chip id; an overlap; hex numbers; an MDF4 head is rejected; 10,001 lines stop with a problem.
- **Stop if:** none expected.

### B2 — `space::kind` and regions

- **Read:** D7, D8; `src/elf/mod.rs:40-130` and `225-260`; `src/map/model.rs`; B1's `RegionTable`.
- **Change:** `src/space/{mod.rs, kind.rs, regions.rs}`:
  - `Kind { Code, Rodata, Data, Bss, Other }`, `of_section`, `of_name`.
  - `Region { name, start: Option<u64>, total: Option<u64>, used, by_kind: [u64; 5], blocks: Vec<Block>, source: RegionSource }` and `Block { start, size, kind, owner: OwnerRef }`.
  - `regions_for_elf(&Elf, Option<&RegionTable>)` follows the D8 source order; clusters split at 16 MiB.
  - `regions_for_map(&MapModel, Option<&RegionTable>)`.
  - `RegionTable::from_map(&MapModel)` and `merge(map, mdf)`.
  - `outside: u64` counts ELF sections outside every region.
- **Check:** `cargo nextest run -p navo-automotive space_regions`. Tests: kinds for every flag mix; `.mdf` regions; `.map` regions with estimated start; clusters with no free figure; a section outside the regions is counted.
- **Stop if:** an ELF section spans two regions; report how many in the fixtures and do not split it.

### B3 — `space::owners_elf`

- **Read:** D9; B2's types; `src/elf/lines.rs` public API; `src/elf/query.rs` (`estimated_size` from A7, or write the same rule here if A7 has not landed and report).
- **Change:** `src/space/owners_elf.rs`: `Owners { items: Vec<Owner { start, size, name: NameRef, kind, object, source_file, section, estimated: bool }>, provenance: Provenance }` built in D9 order; "(not attributed)" fillers; FILE symbols only with real extensions; stripped files give section owners only. `Provenance` names each source and counts estimates.
- **Check:** `cargo nextest run -p navo-automotive space_owners`. Fixtures from `elf_build`: per-function sections; merged sections with LOCAL labels; a global zero-size function estimated; stripped; a sibling MAP model giving objects. The sum of owners equals used bytes per region.
- **Stop if:** the work needs more than 150 changed lines; split fillers into B3b and report.

### B4 — `space::owners_map` and groups

- **Read:** D7 (MAP kinds), D10; `src/map/query.rs` (`object_totals`), B2 and B3 types.
- **Change:** `owners_of_map(&MapModel)`; `Grouping { Module, SourceFile, ObjectFile, Section }`; `group(&Owners, Grouping, region) -> Vec<Group { name, by_kind, bytes, children }>` sorted by bytes; `default_grouping`; `flash_region(&Space)`.
- **Check:** `cargo nextest run -p navo-automotive space_groups`. Tests: the module rule on invented names (`Com_Main.o` → `Com`, `.text.Lamp.Read` → `Lamp`, `_start` → `start`, `.L12` → `(other)`); default falls through when Module yields one group; Object disabled without a MAP; the flash region pick.
- **Stop if:** none expected.

### B5 — `space::strip`

- **Read:** D17; B2's `Region`; the prototype rules restated in D17.
- **Change:** `src/space/strip.rs`: `ScaleMode { Compressed, True }`; `Scale::new(&Region, mode, view: Option<Range<u64>>)` with `x(offset)`, `offset(x)` and breaks; `cells(&Region, &Scale, width_px) -> Vec<Cell { x0, x1, kind, blocks: Range<usize>, bytes }>` merging sub-pixel blocks; `inner(&Owners, block, &Scale, width_px)` for D17 inner cells.
- **Check:** `cargo nextest run -p navo-automotive space_strip`. Tests: compression of a 50 % gap; true scale; merging gives at most one cell per pixel; `offset(x(o)) ≈ o` (proptest); zoom view clips blocks; a cluster region has no breaks.
- **Stop if:** none expected.

### B6 — `space::treemap`

- **Read:** D10.
- **Change:** `src/space/treemap.rs`: `squarify(&[Item { id, value }], Rect) -> Vec<Tile>` (the squarified algorithm), `top(values, 22) -> Vec<Item>` with the "N more" item.
- **Check:** `cargo nextest run -p navo-automotive space_treemap`. Tests: tile areas proportional within 0.5 %; no overlap; tiles cover the rectangle; zero values dropped; 23+ groups give 22 plus "more".
- **Stop if:** none expected.

### B7 — fixtures and budget

- **Read:** D20; `src/elf/sample/mod.rs`, `tests/elf_large.rs`, `tests/elf_build/`.
- **Change:**
  - `tests/elf_build` gains `merged_layout(symbols)` and `split_layout(symbols)` with invented module names (`Com`, `Lamp`, `Seat`, `Horn`, …, 40 modules).
  - Small fixtures: stripped, 32/64, little/big, with and without DWARF, plus invented sibling `demo.map` and `demo.mdf` texts.
  - `tests/elf_rich_large.rs`: `#[ignore]` tests that build both ~30 MB files and time parse, lines, regions, owners, groups, strip cells at 1,600 px, treemap and `select`, asserting the D20 budgets in release builds only (`cfg(not(debug_assertions))`). It prints the timings.
- **Check:** `cargo nextest run --release -p navo-automotive --test elf_rich_large --run-ignored all`; paste the printed timings into the Log.
- **Stop if:** a budget fails by more than 2×; report the timings and do not tune code in this atom.

### B8 — `.mdf` viewer

- **Read:** D16; `viewer/kinds.rs` and `viewer/kinds/` (how ELF and MAP kinds register and sniff), `viewer/map/mod.rs:140-209` (body shape), A2's `columns`.
- **Change:** a new kind `mdf` and `viewer/mdf/{mod.rs, draw.rs}`: parse in a background job, a chips table on `columns`, spaces in the header detail, problems in the status line, `AUTOMOTIVE_TABLE` parts. Add a gallery sample with an invented `demo.mdf`.
- **Check:** `cargo nextest run -p navo-ui viewer::mdf`; gallery shot in both themes.
- **Stop if:** `.mdf` is already claimed by another kind's sniffer; report which.

### B9 — review Part B

Opus high: kind rules, D8 source order and the outside count, D9 owner order and sums, the module rule, strip maths and the budget timings. Then gate, Windows CI and merge (no UI change ships alone except B8).

### C1 — memory-kind tokens

- **Read:** D19; `tokens/mod.rs:90-110` and `230-280`, `tokens/contrast.rs`, `tokens/tests.rs`; `~/.agents/rules/web-ui.md` colour section.
- **Change:** generate the scales with `palette.mjs` (hues 40, 82, 210, 238, 60), add `mem_*` tokens in both themes, add contrast tests for every pair in D19.
- **Check:** `cargo nextest run -p navo-ui tokens`; `contrast.mjs` output pasted in the Log.
- **Stop if:** a pair cannot reach its ratio inside the hue family; report the pair and the best step.

### C2 — `AUTOMOTIVE_LAYOUT` parts and settings

- **Read:** D5; `viewer/parts/mod.rs:1-120`, `parts/tests.rs`, `viewer/view_menu.rs:143-172`, `settings/fields.rs:90-100` and `400-415`, `settings/viewer_fields.rs:62-110`, `settings/values.rs:266-280`, `commands/viewer.rs`, `navo-ui-gallery/src/viewer_menu_page.rs:140-160`, `settings.ftl`.
- **Change:** fields `AutomotiveUsageBars` and `AutomotiveWhoUses` with keys `automotive.view.usage-bars` and `automotive.view.who-uses`, default off; actions `ToggleUsageBars` and `ToggleWhoUses`; the `AUTOMOTIVE_LAYOUT` set; `family` widened to all six fields for every automotive set; ELF and MAP bodies return `AUTOMOTIVE_LAYOUT`; `show_parts` passes a `LayoutParts { strip, bars, who, row_bars }` struct to both bodies (stored, not drawn yet).
- **Check:** `cargo nextest run -p navo-ui parts settings viewer::tests::view_parts`, then `cargo xtask gate`. Tests: Rich turns all on; a toggle makes Custom; Keep as default writes the two new keys; the View menu lists five rows for ELF and MAP and four for HEX.
- **Stop if:** the store's settings schema needs a migration for the new keys.

### C3 — `space::sibling` loader

- **Read:** D8; `viewer/files.rs:100-130`, `viewer/background.rs:1-80`, `viewer/elf/jobs.rs:37-75`, `navo-core/src/location/mod.rs:77-100`, `location/parent.rs`; B1 and B2 APIs; `navo_automotive::map` reader entry (`src/map/mod.rs`).
- **Change:** `viewer/space/{mod.rs (stub), sibling.rs}`: `start_siblings(host, stem_of(location)) -> Job` that tries the D8 names through `hooks.bytes` with the limits, parses the `.mdf` and the `.map` (TASKING sniff first), and sends `Siblings { mdf: Option<Mdf>, map: Option<Arc<MapModel>>, names, problems }`. Unreadable files count as absent.
- **Check:** `cargo nextest run -p navo-ui viewer::space::sibling`. Tests with fake hooks: `.mdf` only; `.MAP` upper case; a non-TASKING `.map` ignored; a 65 MiB `.map` refused; no sibling.
- **Stop if:** `hooks.bytes` reads the whole file before checking the limit; report and use `hooks.range` with a size probe only if the brief's owner agrees.

### C4 — bars, legend, File bytes

- **Read:** D2, D19; prototype rules for `ubar`, `uline`, `legend` and `fileBar` restated in D2; `rich_header.rs`; C1 tokens; B2 `Region`.
- **Change:** `viewer/space/bars.rs`: `UsageBar` (kind segments over a free track, accessibility label "name: N % used, X free"), `UsageLine`, `Legend`, `FileBytesBar` (loaded, DWARF, notes, symbols and strings, other, from `Elf.sections`), `RegionHead` (name, large percent). A cluster region shows "free not known" text instead of free figures. Gallery page with an invented three-region file.
- **Check:** `cargo nextest run -p navo-ui viewer::space::bars`; gallery shots at 390 px and desktop width in both themes.
- **Stop if:** none expected.

### C5 — `MemoryStrip` element

- **Read:** D17; B5 API; `primitives/tooltip.rs`; a custom-paint example in the crate (`grep -rn "canvas(" crates/navo-ui/src`); C1 tokens.
- **Change:** `viewer/space/strip.rs`: paints cells (one quad per cell), break marks, inner cells, the caret and dimmed cells. Hover maps x to a block and shows the tooltip. A click emits `StripEvent::Pick(block)`. Focus ring when focused. Cells come from a cache keyed by D17.
- **Check:** `cargo nextest run -p navo-ui viewer::space::strip`. Tests: hover at x shows the right block; click picks; dimmed set; caret position; cache reused when only the selection changes. Gallery shots: compressed, true scale, zoomed, merged sub-pixel.
- **Stop if:** GPUI cannot paint 2,000 quads per strip within one frame in the gallery; report the frame time.

### C6 — `Treemap` element

- **Read:** D10; B6 API; C1 tokens; A1 `fit_name`.
- **Change:** `viewer/space/treemap.rs`: tiles as absolutely placed elements with name, size and share when large enough, inner tiles, a kind bar, a "N more" tile, a selected outline in the accent colour, and `TreemapEvent::Pick(group)`. A stripped file shows the D9 note.
- **Check:** `cargo nextest run -p navo-ui viewer::space::treemap`; gallery shots at three sizes, both themes.
- **Stop if:** none expected.

### C7 — Rich container in the ELF view

- **Read:** D2–D6, D10, D13, D17; `viewer/elf/mod.rs`, `elf/draw.rs`, `elf/jobs.rs`, `elf/drawers.rs:1-80`; C2–C6 APIs; C3 loader.
- **Change:**
  - `viewer/space/mod.rs` and `layout.rs`: `RichSpace` state per open file (scale mode, zoom, grouping, selection, focused part) and the D2 layout, gated by `LayoutParts` and D3.
  - The Rich model (regions, owners, groups) is built in the ELF index job after parsing, and rebuilt when the siblings arrive.
  - The provenance line, the noise bar above the table, and the Share column when row bars are on.
  - The inspector shows blocks, groups and symbols with full names; the status line shows the selection's full name.
  - Simple renders exactly as before; a test compares its element tree with the pre-P38 snapshot.
- **Check:** `cargo nextest run -p navo-ui viewer::space viewer::elf`. Screenshots of an invented ELF in Simple, Rich and Custom (each part off once) at 390 px, 1,280 px and 1,920 px, both themes, with the inspector open over the content.
- **Stop if:** `elf/mod.rs` or `elf/draw.rs` would pass 400 lines; move the Rich branch into `viewer/space/layout.rs` and report.

### C8 — Rich container in the MAP view

- **Read:** C7's diff; `viewer/map/mod.rs`, `map/draw.rs`, `map/jobs.rs`, `map/loaded.rs`.
- **Change:** reuse `RichSpace` and the layout with MAP owners, Object file grouping, "reserved" in the usage line, and Located sections in the table.
- **Known gap (B9 review):** D7's "MAP kind from the sibling ELF section with the same name and address" is not built. `regions_for_map` and `owners_of_map` take no ELF and C3 loads no sibling ELF, so MAP kinds come from section names only and the provenance line says "kinds from section names". Add the ELF parameter here if the MAP view should show ELF kinds.
- **Check:** `cargo nextest run -p navo-ui viewer::space viewer::map`; the same screenshot set with the invented gallery map.
- **Stop if:** the MAP view needs a container change that C7 did not provide; report rather than fork the container.

### C9 — strip, treemap and Alt keys

- **Read:** D18; `viewer/elf/keys.rs`, `viewer/map/keys.rs`, `commands/keymap.toml`, `commands/viewer.rs`, `commands.ftl`, `viewer/body.rs:60-125` (`keys()` hints).
- **Change:** `viewer/space/keys.rs` with the D18 actions in a `space_view` namespace for the Elf and Map contexts; registry rows, `cmd-` labels, F-key bar hints per focused part; Backspace zooms out only when a zoomed strip has focus.
- **Check:** `cargo nextest run -p navo-ui viewer::space::keys commands`, then `cargo xtask gate`. Tests: Tab cycle; arrows move the selection and the inspector follows; Enter zooms and Backspace zooms out; Backspace on the table still edits the filter; Alt+G, Alt+Z and Alt+H; letters still type into the filter; `bound_keys.snap` shows only the new rows.
- **Stop if:** Tab is consumed by the focus system before the Elf context sees it.

### C10 — large-file UI check

- **Read:** D20; B7 fixtures; `crate::test_window`.
- **Change:** `viewer/space/large.rs` (test only): an `#[ignore]` release test that opens both ~30 MB fixtures in a headless window, waits for the index, switches to Rich, and times the first Rich frame and ten selection moves along a strip.
- **Check:** `cargo nextest run --release -p navo-ui viewer::space::large --run-ignored all`; timings in the Log.
- **Stop if:** a frame exceeds 2× the budget; report where the time goes (model, cells or layout) without tuning.

### C11 — review Part C

Opus high on C1–C10: Simple unchanged, drawers over the content, provenance truthful, the free figure absent for clusters, colour pairs, keyboard paths, and no model work in `render`. Then gate, Windows CI, merge, an alpha build and the owner check on a Windows PC: both real ELF files, the real MAP and `.mdf`, HEX and MOT, at the work monitor's width.

## Questions for the owner (answered 2026-10-20)

- Q1: "The best way to cover everything": the user's list, then AUTOSAR two-part prefixes, then the first `_` or `.` (D10).
- Q2: "Usually yes, all 5 are generated in the same folder": same stem first, then the only `.mdf` or `.map` in the folder (D8).
- Q3: "Ok": the table changes apply in Simple too.

The original questions:


- **Q1 — Module names.** Is "the text before the first `_` or `.`" right for your module names, or do you want an editable prefix list in `navo.toml` (for example `[automotive] modules = ["Com", "Dio_Cfg"]`)?
- **Q2 — Where the `.map` and `.mdf` live.** Are they always beside the `.elf` with the same name? If not, where do your builds put them?
- **Q3 — Simple gets three table changes.** Resizable columns, names that keep their prefix, and the zero-size filter (with "N hidden" in the status line) apply in Simple too. Is that what you want, or should Simple keep today's fixed columns and every symbol?

## Done when

- Rich on both real ELF files shows regions with used and free bars, strips and a treemap that adds up to the used bytes; the provenance line names the sources. Checked by C11 on a Windows PC.
- An ELF without a sibling shows clusters with "used" only; a stripped ELF shows the note. Checked by C7 tests.
- Simple renders as before P38. Checked by the C7 snapshot test.
- Columns resize in ELF, MAP, trace, CSV and `.mdf` tables. Checked by A3–A6 and B8.
- The MAP status names the unread parts; Space is filled; Size hides when absent. Checked by A9 and A10.
- The owner's HEX shows 2 ranges with "196 folded" and 32-byte rows on the work monitor. Checked by C11.
- The budget tests pass in release (B7, C10). Gate and Windows CI green for each part.

## Log

- 2026-10-20 — Plan written. Owner picked direction 1 "Address space first" from the Rich ELF/MAP prototype. Measured the owner's files (counts above). Found: TASKING Symbols tables have no size column; Space is printed only when it changes; "N other parts not shown" counts about 955 call-graph lines as part headers; the HEX has 196 gaps of 16 bytes or less. A second 26 MB ELF uses merged sections and about 95 % zero-size labels, so owners come from symbols (D9) and the strip draws symbols inside wide sections (D17). Orchestrator rule D6: Rich never assumes one toolchain's layout.
- 2026-10-20 — A3 done: the ELF symbol table renders through `columns`; `ColumnSet` gained `set_measured` and `text_chars` (flex column width for `fit_name`).
- 2026-10-20 — A12 done: HEX/MOT view folds gaps under 256 bytes and uses 32-byte rows when the width fits.
- 2026-10-20 — B3 done: `space::owners` (owners_elf). Rules: MAP contributions, section names for per-function layouts, sized symbols, estimated globals, "(not attributed)" fillers, `.data` load image mirrored at its LMA. The ELF parser now keeps FILE marks (`Elf::file_marks`) because it dropped FILE symbols before.
- 2026-10-20 — Probe on the owner's files with B3: owner sums equal `Region.used` in every region, 1.4–3.0 ms. TASKING ROM copies of initialised data appear as their own `[.data.…]` sections in flash, so ROM usage already counts them (no LOAD-segment image needed). demo.elf has 9 `$fill` sections (671,680 bytes in one flash region) that showed as the largest owner and counted as used; D7 now adds the Fill kind (atom B2b).
- 2026-10-20 — B2b done: `Kind::Fill` (ELF sections named `$fill*`, MAP entries `*fill*` or `$fill*`). `Region.used` leaves Fill out, `by_kind` and blocks keep it, owners skip Fill sections so owner sums still equal `used`.
- 2026-10-20 — B5 done: `space::strip` has `Scale` (gap compression, true scale, zoom view, breaks) and `cells` (sub-pixel merge, at most one cell per pixel column, 1 px minimum). `inner` for D17 inner cells waits for B3 owners.
- 2026-10-20 — A4 done: the MAP tables (Memory, Sections, Symbols) render through `columns`, one `ColumnSet` per table kept while the file is open; section names use `map_prefix`, symbols `symbol_prefix`.
- 2026-10-20 — B4 done: `owners_of_map` (one owner per located contribution, gaps "(not attributed)", Fill skipped), `group`/`Grouping`/`GroupBy`/`Origin`, `top`, `available`, `default_grouping`, `flash_region`, `Region::span`. Decisions taken: the module list is a `&[String]` parameter (the `navo.toml` setting is wired in C); module candidates are object stem, then section name without its kind part, then owner name, and `.L<caps/digits>` labels leave nothing; "(other)" for missing keys, "(not attributed)" keeps its own group in Module and Object; groups count load images in `bytes` and `by_kind` and report them in `image`; children are object files for a MAP Module group, owner names otherwise (all of them, sorted; the UI cuts at 14); `top` cuts groups at 22 with "N more" (B6 `top` works on items); `flash_region` ranks rom chips with code first, then code+rodata, then used; MAP section names lose their ` (n)` suffix before the kind lookup (fixes `regions_for_map` kinds on TASKING maps).
- 2026-10-20 — B6 done: `space::treemap` (`layout(&[Item], Rect) -> Vec<Tile>`): squarified layout, top 22 plus one "N more" tile (last), zero weights dropped, order by size then input order, text flag above 64 × 34, inner tiles above 150 × 110 (top 14 parts plus "N more", laid out below an 18 px header and above the 4 px kind bar). Release timing for 22 × 14 tiles is under 1 ms.
- 2026-10-20 — B5b done: `space::inner(&Owners, &Block, &Scale, width_px)` returns inner cells for one block from its owners, in strip pixel coordinates; empty when the block is 64 px wide or less or holds fewer than two owners. It shares the `cells` merge rules (1 px minimum, dominant kind, no overlaps); `Cell.blocks` indexes `Owners.items`.
- 2026-10-20 — B7 done: generated split ELF (29.36 MB, 3,317 sections, 3,309 LOAD segments, 56,000 indexed symbols, nine `$fill` sections), merged ELF (29.60 MB, 111 sections, 50 LOAD segments, 99,000 indexed symbols, 88,000 zero-size LOCAL labels, 5,000 sized symbols), synthetic MAP/MDF and 16 small class/endian/stripped/DWARF variants with sibling combinations. The existing `object` writer supports owned section names in test support; non-allocated synthetic payload sets file size. Release timings, split/merged in ms: parse 2.107/2.806, DWARF 23.923/8.103, combined 26.031/10.909; Rich ELF model 6.857/8.769, split ELF+MAP 4.821, MAP 3.775 (MAP parse 3.614); strip maximum 0.056/0.003 at 1,600 px in both scales; treemap maximum 0.092/0.148 across all groupings; `select` with `hide_noise` 0.376/4.264; peak RSS including generation 72.016/79.667 MB on Linux. All budgets and owner/group sums passed; owner sums exclude load-image copies, group sums include them. The timing helper uses `black_box`; budgets assert only in release. Three release tests passed; gate ended "all 10 steps passed" (6,214 workspace tests passed, 11 skipped).
- 2026-10-20 — C1 done: `Theme.mem` (`tokens/memory.rs`) holds strong, tint and text per kind plus `free` and `free_line`. contrast.mjs: strong on background light 3.73-4.05:1, dark 6.48-6.94:1; text on tint light 5.81-6.24:1, dark 8.33-8.37:1; text on background light 6.74-7.31:1, dark 11.69-12.04:1; free_line 3.86:1 light, 4.53:1 dark.
- 2026-10-20 — C2 done: `AUTOMOTIVE_LAYOUT` (five parts) for ELF and MAP, settings `automotive.view.usage-bars` and `who-uses` (default off, store fields, ledger count 44), toggle actions and commands, `LayoutParts { strip, bars, who, row_bars }` stored on both bodies (not drawn yet). `family` is all six fields for every automotive set. No migration needed.
- 2026-10-20 — C5 done: `MemoryStrip` entity (`viewer/space/strip/`) paints cells, breaks, inner cells, dimmed cells and a caret from a layout cached per width (8 px steps), scale mode and zoom; caret, lit set and hover reuse it. Hover shows the block, merged count or inner symbol in the tooltip and emits `Hover`; a click focuses and emits `Pick`. `step`, `zoom_in` and `zoom_out` (false when not zoomed) are ready for C9's keys; steps stay inside a zoomed block. All quads go in one paint layer: 2,096 cells paint in about 0.7 ms (debug), 125 ms without the layer. Gallery page `p38-c5-memory-strip`.
- 2026-10-20 — C6 done: `Treemap` element (`viewer/space/treemap.rs`) draws `layout` tiles placed absolutely: tint and text from the biggest kind in `Theme.mem`, a kind bar of strong segments, text only where the layout flag is set, inner tiles with part names, a neutral "N more" tile last that picks nothing, an accent outline for the selected group, `TreemapEvent::Pick(group)` on click, and the D9 note when stripped. The caller gives `TreemapGroup` names, kind bytes and part names by id. Gallery page `p38-c6-treemap` shows three sizes. Texts are in `viewer.ftl`.
- 2026-10-20 — C3 done: `viewer/space/sibling.rs` finds `<stem>.mdf`/`.MDF` and `<stem>.map`/`.MAP` beside the open file, probes size and sniffs with `hooks.head` before `hooks.bytes` (1 MiB, 64 MiB), never loads the open file itself, and reports too large, not recognised and unreadable siblings as typed problems; the "only .mdf here" fallback waits for a folder-listing hook. 13 tests; `xtask check -p navo-ui` ended "all 7 steps passed".
- 2026-10-20 — C3 review (orchestrator): D8's fallback (the only .mdf/.map in the folder) is not built; `FileHooks` cannot list a folder. C7 adds a folder-list hook or leaves the fallback for later and says so.
- 2026-10-20 — C4 done: `viewer/space/bars.rs` has `UsageBar` (kind segments over a free track, linker fill as a tint, label "name: N % used, X free"), `UsageLine` (used, free, reserved, total; a cluster says "free not known" and shows no free figure), `RegionHead` (name, large percent), `Legend` (reuses `space-kind-*`, adds free) and `FileBytesBar` (`FileBytes::of(&elf.sections)`: loaded, DWARF, notes, symbols and strings, other; NOBITS and null sections have no file bytes). Gallery page `p38-c4-bars`. Fill is not hatched yet.
- 2026-10-20 — C4 review (orchestrator): for C7/C8: linker fill must be hatched in the bars (D7; it is a plain tint now); percent is written without a space ("62%", as D67's amendment); File bytes reuse kind colours, so keep its named chips.
- 2026-10-20 — C4 review (orchestrator): for C7/C8: linker fill must be hatched in the bars (D7; it is a plain tint now); percent is written without a space ("62%", as D67's amendment); File bytes reuse kind colours, so keep its named chips.
- 2026-10-20 — A8 done: the ELF status line shows "N hidden (zero-size, ABS)" in every mode; Alt+H (`elf.toggle_noise`) switches the filter per file; Ctrl+G to a hidden symbol still opens it in the inspector and sets the D13 note. Keymap count 363 to 364. Three UI tests.
- 2026-10-20 — B9 done (Opus review, fixes round): load images are Data blocks in `regions_for_elf` (`OwnerRef::ElfImage`), so flash `used`, free and the strip count them and group sums equal `used`; an image that overlaps a placed section or another image is dropped, so TASKING `[.data.X]` ROM copies count once. `[.data.X]` loses its brackets for names and modules (`plain_section`) and is rodata by name. Each region groups only the owners inside its own placed blocks, so overlapping or aliased regions no longer count owners twice; a chip mapped more than once gets " (2)", " (3)" names. Load-image addresses use `checked_add`; an overflowing image is skipped. `group` skips the section-name candidate for symbol owners. Kept `is_copied_data` (NOBITS writable `.data/.zdata/.sdata` is data): it came from real TASKING files, and D7 now says so. Gap for C8: D7's "MAP kind from the sibling ELF" is not built (MAP functions take no ELF). Not done: no-axis-labels for a region with unknown start (the strip has no axis labels until C5/C7 draws them; show "start not known" there). New release fixture: 210,000 symbols with 4,000 zero-size GLOBAL FUNC symbols times the estimate path (Rich model 13 ms, budget 150).
- 2026-10-21 — C7 done: `viewer/space/{model,rich,layout,switch}.rs` and `elf/{rich,rich_view}.rs`. The model (regions from `.mdf`, `.map` or clusters; owners; flash groups; provenance line) builds in a background job after parsing and again when the siblings or DWARF lines arrive. Rich and Custom draw the address-space panel (legend, Compress gaps / True scale, region head, used bar, usage line, strip), File bytes with its named chips, and "Who uses the flash" (grouping switch with disabled reasons, treemap) beside the table, stacked under 1,100 px; the top panel scrolls inside 55 % of the height so the table keeps rows. The table gains the noise bar and a Share column (symbol size over its region's used bytes) and sorts largest first while Rich is on, unless the user sorted. Picking a strip block or a treemap group sets the inspector and the status line to the full name; a group dims strip blocks outside it (`space::lit_blocks`, new). Sections outside the sibling's regions show in the status line in every mode. Linker fill is hatched in bars and legend; percent has no space. Not done: the D8 "only .mdf/.map in the folder" fallback (no folder-list hook); strip axis labels (none drawn, so no "start not known" text yet); the user's `[automotive] modules` list (no setting exists; groups use `&[]`); focus part and keys (C9). The Simple check is behavioural (no Rich ids, address order, old tests unchanged), not an element-tree snapshot: the test kit has no tree dump. Gallery pages `p38-c7-elf-*` at 1,280 × 720 only.
- 2026-10-21 — C8 done: `map/{rich,rich_view}.rs`; the MAP view reuses `RichSpace`, `rich_layout` and the treemap through `RichHost`, with owners from the map's own contributions (Module groups by object stem; Object file and Section are in the switch; Source file is disabled, as a map has no DWARF). Rich and Custom switch a default MAP table to Located sections, largest first, with the Share column (section size over its region's used bytes); leaving Rich restores Memory unless the user changed the table or sort. `Region.reserved` (new; from the map's Memory Usage, summed per region) is shown as "reserved" in the usage line and taken out of free, for the MAP and for an ELF with a sibling `.map`. The inspector and status line name a picked block or group in full; "N sections outside the regions from X" shows in the status line. Gaps done: D7 "MAP kind from the sibling ELF": `regions_for_map` and `owners_of_map` take `Option<&Elf>` (one parameter, no model change), and a MAP opened next to `<stem>.elf` or `.ELF` loads it (64 MiB limit, same problems as the other siblings) so a located section takes the kind of the ELF section with the same name and address, else its section name; the provenance line says "kinds from <name>" or "kinds from section names" and does not count how many sections matched. The D8 fallback (the only .mdf/.map in the folder) is still not built: `FileHooks` has no folder-list hook. Shared pieces moved out of the ELF view into `space/facts.rs` (inspector facts, Share cell); the Share cell gained a right gutter for the scrollbar. Fixed on the way: a MAP opened in Rich started on a wrong row because the first select ran over the default table's rows. Not done: the user's `[automotive] modules` list (no setting), focus part and keys (C9). Gallery pages `p38-c8-map-*` at 1,280 × 720 only.
- 2026-10-21 — C8 review (orchestrator): for C9: show "reserved" in the usage line only when it is above 0; the provenance line should say "kinds from <elf>" only when an ELF section matched.
- 2026-10-21 — C9 done: `viewer/space/keys.rs` adds 13 `space_view` actions (`space.*` commands, `cmd-space-*` labels) in a new `Space` context that the ELF and MAP roots carry next to `Elf` and `Map`. Tab and Shift+Tab cycle table, each region's strip and the treemap; Alt+G picks the next usable grouping; Alt+Z switches the scale. In a strip (`SpaceStrip`), ←/→, Home and End pick blocks and the inspector follows, ↑/↓ move to the region above or below, Enter zooms in, and Backspace zooms out only when the strip is zoomed, else it edits the filter. In the treemap (`SpaceTreemap`), the arrows move a focus ring in reading order and Enter selects the group. Letters still type into the filter from every part. The F-key bar shows the keys of the focused part. Navigation commands are hidden from the palette; Alt+G and Alt+Z stay. A regroup keeps the strip entities, so focus stays put. From the C8 review: "reserved" is shown only above 0, and "kinds from <elf>" only when an ELF section matches (`space::elf_kinds_match`). Gallery page `p38-c9-elf-keys`. Not done: View-menu rows for the keys (D18 asks for them; this atom's change list does not); strip axis labels.
- 2026-10-21 — C10 done: `viewer/space/large.rs` (`#[ignore]` release test) builds the merged (99,000 symbols) and split (56,000 symbols) ~30 MB fixtures with the B7 generators, opens each in the headless window, waits for the index, switches to Rich and times the frames and ten `right` presses along the first strip (each press includes two frames and the settle). Release ms, merged/split: open and index in Simple 69.0/90.9; switch call 13.1/12.6; first Rich frame 3.6/3.8 (budget 100); model job and settle 25.7/34.2; steady Rich frame 3.7/3.7 (budget 100); strip move mean 27.6/27.3 and worst 28.2/27.9 (D20 has no move budget; the 100 ms frame budget is shown). All within budget; nothing to tune. The test asserts the first Rich frame only, in release builds. It uses the B7 files through `#[path]` and a new `object` dev-dependency; `open_with_mdf` (new, in the viewer test helpers) serves the fixture's own `.mdf`. Only the two ELF fixtures are timed; no MAP-view timing.
- 2026-10-21 — C11 fix, bars and numbers: percent floors ("99%" until full, "<1%" above 0, "0%" at 0); a reserved segment (neutral hatch) sits between used and free and the legend names it only when a region has reserved bytes; a region without a known total draws a thin composition bar with no free track; a zero-size region shows its used size and no percent; the inspector shows exact bytes through `format::exact_bytes` ("91,592 B"; `format::bytes` already existed as the grouped count and is used for item counts). The inspector shows only size, so used, free and reserved have no exact figure to change.
- 2026-10-21 — A10 done: the MAP status says "Not read: <first three titles> +N more" and "This map lists no symbol sizes"; the navigator ends with a "Not read" group (`NavItem::NotRead`, `Unread`) that cannot be selected, and arrow keys stop before it; the Symbols table drops Size when `has_symbol_sizes()` is false (Simple and Rich share the table). New gallery page `p38-a10-map-unread`.
- 2026-10-21 — C11 fix (treemap): every inner tile now has colour, a kind bar and a "size · share" caption when large enough. Colour is by group (D10 amendment): `space/treemap/palette.rs` makes one hue per group and a shade per part rank, one ink per theme (contrast 7.1:1 or better in both themes). All children now go to the layout (`Item.parts`, no `take(14)`), so it cuts to 14 plus "N more" and areas are true; `Child.by_kind` is new in the model. Zero-size GLOBAL symbols with a D9 estimate show "~x.x%" in the Share column and Rich sorts by it (`Column::Share`, `Boundaries` is public). Gallery MAP demo gains 17 `com_unit` objects (regions grown to fit). Not done: the ELF demo has no group with inner tiles (too few symbols).
- 2026-10-21 — B8 done: `viewer/mdf/{mod,draw,jobs,keys,text}.rs` and `kinds/mdf.rs` (kind `memory-definition`, extension `.mdf` and `sniff_mdf`, score 70, Rich slot; nothing else claimed `.mdf`). The parse runs in a background job over the first 2 MiB. One table on `columns` with Chip, Type, Size, Space, Start in space, End and Start on chip, a row per mapping and a row for a chip without one. The header detail lists the spaces, the status line shows the first three parse problems and "+N more", then the chip and mapping counts. `AUTOMOTIVE_TABLE` gives the Ctrl+I inspector with the selected row; `Mdf` is a new `Viewer` child context with Esc bound to back. Not done: sorting by column. 5 tests in `viewer::mdf`, 4 window tests in `viewer::tests::mdf_view`. Gallery page `p38-b8-mdf-table` with an invented `demo.mdf` (one unknown chip and one bad number on purpose) at 1,280 × 720.
- 2026-10-21 — C11 fixes (keys and state): `RichSpace::install` reuses strip entities by index, drops only the tail, and moves a lost focus to the table through a deferred window update; a rebuild keeps a strip's zoom and caret when its blocks are unchanged, keeps a block pick, and keeps a group pick when the grouping is the same and the group name still exists (its dimming is redone). The treemap tiles are laid out again at the same size, so the treemap element and its focus survive a regroup. Alt+G now runs only the grouping (`space/regroup.rs`, a second job in each view) on the model it has; a full build that lands with another grouping triggers one more regroup. Esc clears a pick before it closes a drawer, clears the filter or leaves the viewer. A region without blocks has no Tab stop and the up and down keys skip it. Tests in `space/keys/rebuild.rs`.
- 2026-10-21 — Owner on 21c with a real ELF (beta or after): (1) a picked treemap tile must not turn solid blue; show the pick as a border and keep the tile's colour and text. (2) Drill into a group: open the treemap on one module (for example ReProg) so its object files fill the map, instead of a dark "333 more" tile and unlabelled slivers. Also seen: "N more" can take most of a group when the group has hundreds of small parts.
- 2026-10-23 — D8 sole-companion fallback is implemented on backend/rename-undo-safety. FileHooks can discover candidates in the background; both lookups share one bounded, cancellable scan. Same stem wins; multiple candidates stay unchosen; body limits and format sniffers still apply; a different-stem ELF is never guessed. The provenance identifies a sole MDF or MAP. Configured module prefixes and native Windows verification remain open.
- 2026-10-23 — D9 configured prefixes are implemented on the same backend branch. `[automotive] modules` is a typed list, editable through the existing settings control and reloaded live. ELF/MAP model builds, regrouping and block highlights retain the same prefix snapshot; stale results are rejected. Provenance counts matched configured modules. Headless ELF and MAP live-regroup tests and the live file-reload test pass; native Windows checks remain open.
- 2026-10-26 — Final review fixes: MAP section names keep their kind in the muted prefix (`.text.Com.`; D12 amended), and a short width shortens only the module part. The region panel scrolls the focused strip into view. Duplicate `.mdf` chip names are numbered in linear time. Name cells measure the UI font and end with an ellipsis when they still overflow. The provenance line names both files when the `.map` gives the regions and the `.mdf` the starts. The noise bar counts only the symbols that match the filter. Sums near `u64::MAX` saturate. File bytes and the measured character widths are computed once, outside render. "start estimated" is not shown: no place in the layout shows a region start yet.
- 2026-10-26 — Final review (Opus medium) for A13 and C11: fix first. Fixed on fix/rich-elf-map-review: MAP names keep their kind, focused strips scroll into view, linear .mdf duplicate naming, name width measured in the UI font with an ellipsis, two-file provenance, saturating sums, no model work in render. A5 and A6 (CSV and CAN trace column resizing) move to the beta. Next: Windows CI, merge, owner check on a Windows PC.
