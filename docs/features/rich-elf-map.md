# Rich ELF and MAP views

Status: the Rich panels are in for ELF and MAP. The `.mdf` table is in.

## What it is

Rich mode in the ELF and MAP viewers shows where memory goes. Simple mode stays as it is today.

## What you can do

- See used and free space for each memory region, with a legend.
- See a memory strip in address order. Switch between "Compress gaps" and "True scale".
- See the bytes of the ELF file as a bar.
- See "Who uses the flash" as a treemap, next to a table of symbols or located sections.
- Group by Module, Source file, Object file or Section. Pick a group to dim the rest of the strip.
- Open a TASKING `.mdf` file as a table of chips.

Under 1,100 px the columns stack.

## How it works

- Any ELF works, whatever the toolchain: per-function sections, merged sections or stripped.
- A line names where the numbers came from. Region totals come from a `.mdf` beside the file, else a `.map`, else address clusters. With clusters only used space is known, and Navo says "free not known".
- A single `.mdf` or `.map` in the folder is used even with a different file name. The source line identifies it as the only matching file. Multiple candidates are left unchosen.
- Those files load in the background. Simple never waits.
- Kinds are code, rodata, data, bss, other and Fill.
- Owners come from MAP objects, per-function sections and sized symbols. Zero-size global or weak symbols get an estimate. The rest is "(not attributed)". A stripped ELF shows a note instead of the treemap.
- Module names use the longest configured prefix in `navo.toml` (`[automotive] modules = [...]`), then AUTOSAR `Rte_` and `SchM_` prefixes, then the text up to the first `_` or `.`. The Module prefixes setting edits the same list. Changes regroup open ELF and MAP views in the background; the source line counts modules matched from the list.
- The treemap shows the top 22 groups and an "N more" tile.

## Tables

- Columns resize by drag. Double-click fits a column.
- Long names keep their prefix. The inspector always shows the full name.
- This holds in the ELF, MAP, CAN trace, CSV and `.mdf` tables.
- ELF: "Hide zero-size and ABS" is on by default. The status line shows "N hidden". Ctrl+G still finds a hidden symbol.
- MAP: part headers are detected more strictly. "Not read: ..." lists unread parts. A blank Space cell repeats the one above. The Size column hides when the map has no sizes.
- HEX and MOT: padding gaps under 256 bytes fold into the rows. Rows are 32 bytes wide on wide screens, else 16.

## Keys

- Tab and Shift+Tab move focus between the table, the strips and the treemap.
- Left and Right move between strip blocks. Up and Down move between regions. Home and End jump.
- Enter zooms into a block. Backspace zooms out. Esc clears a group selection.
- Alt+G picks the next grouping. Alt+Z compresses or shows true scale. Alt+H hides noise.
- Ctrl+B opens the navigator. Ctrl+I opens the inspector. Alt+V opens the View menu.

## Settings

Custom mode lets you turn parts on or off: the inspector, the memory strip, the used and free bars, "Who uses the flash" and the row bars.

## What was decided

- Memory-kind colours work in both themes and pass contrast checks. Colour never carries meaning alone.
- Drawers stay over the content.
- Budgets, logged and not gated: parse about 300 ms, Rich model 150 ms, treemap layout 1 ms.

## Done and coming

- Done: the data behind the panels, the shared resizable columns, the ELF, MAP, CSV and CAN tables, the noise filter, the MAP fixes, HEX and MOT folding, the `.mdf` reader and table, the memory-kind colours, the View-menu parts, the Rich layout, and the strip and treemap with their keys.
- Coming: the `.mdf` viewer polish.

## Open questions

Module grouping was answered: cover every case. The plan lists no other blocking question.

Refs: plans/2026-10-20-p38-rich-elf-map.md (in git history) (D1 to D22)
