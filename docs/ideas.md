# Navo ideas

A backlog of ideas that are not in a plan yet. Each becomes a plan when its turn comes in the [roadmap](roadmap.md).

Every idea starts with one line that Cockpit's Roadmap tab reads: `Status: <idea | accepted | planned | building | shipped | deferred | cut> · Project: navo · Theme: <…> · Needs: <ids, or —>`. `module-registry` means roadmap v1.0, item 5.

## The automotive module (D112)

After the module registry ships, everything automotive and embedded moves into one module that users can switch off. Not every user wants it. It takes the ELF and MAP viewers, simple and rich ([rich-elf-map](features/rich-elf-map.md)), HEX and S-record, CAN traces with DBC, MDF4, A2L, and the m4, OIL and DCM colouring. The module's name is proposed as "Automotive", with "embedded" in its description.

### N1 — Memory delta between two builds

Status: accepted · Project: navo · Theme: automotive · Needs: module-registry

Two MAP or ELF files side by side. It shows which module grew or shrank in flash and RAM, and by how many bytes. An optional budget warns, for example "RAM above 90%". It reuses the work of [compare](features/compare.md) where it fits.

### N2 — HEX and S-record compare by address

Status: accepted · Project: navo · Theme: automotive · Needs: module-registry

The blocks that differ between two images, and a CRC over an address range, as a bootloader computes it.

### N3 — Symbols in Ctrl+K

Status: accepted · Project: navo · Theme: automotive · Needs: module-registry

Type a function or variable name and get its address, size and build, across build folders.

### N4 — Address to source

Status: accepted · Project: navo · Theme: automotive · Needs: module-registry

Paste an address from a crash or trap and get the function and `file:line` from DWARF, like `addr2line`.

### N5 — CAN signal chart

Status: accepted · Project: navo · Theme: automotive · Needs: module-registry

On top of the DBC decoding, pick 2–3 signals and chart them over time, with keyboard zoom.

## Core and other modules

### N6 — Key trainer

Status: accepted · Project: navo · Theme: keyboard · Needs: —

Navo notices, locally, actions done by mouse or menu that have a key, and says so once: "Copy from the menu 12 times; F5 does it." It can be switched off.

### N7 — Local usage statistics

Status: accepted · Project: navo · Theme: keyboard · Needs: —

Opt-in. Which commands the owner uses every day and which features were never touched, so the roadmap is ordered by real use. Nothing leaves the machine.

### N8 — Folder notes

Status: accepted · Project: navo · Theme: workflow · Needs: —

A small Markdown note attached to a folder, for example "this is the release candidate for X", shown in the drawer.

### N9 — Huge-file mode

Status: accepted · Project: navo · Theme: workflow · Needs: —

Open a log or CAN trace of several GB at once: memory-mapped, line offsets indexed, jump to a time, search inside.

### N10 — A Navo track in Cockpit's Study tab

Status: accepted · Project: navo · Theme: learning · Needs: —

Navo's own code as Rust and algorithm lessons: the crash-safe journal (a write-ahead log), the file index layout (2 million entries in about 97 MiB), Ctrl+K fuzzy scoring, shared listings (`Arc` and copy-on-write), the NTFS USN journal, and mimalloc (allocators). Tracked in Cockpit's `plans/2026-11-09-cockpit-next.md` (I13, I15).

## Waiting for a yes

Proposed on 2026-11-08; the owner has not accepted these yet.

### N11 — Where a file came from

Status: idea · Project: navo · Theme: workflow · Needs: —

Windows keeps the download URL in the `Zone.Identifier` stream. The inspector shows "Downloaded from X".

### N12 — Copy paths as…

Status: idea · Project: navo · Theme: workflow · Needs: —

Copy the selected files as quoted paths, a PowerShell array, a CMake list or a Python list.

### N13 — Send to USB, safely

Status: idea · Project: navo · Theme: workflow · Needs: —

Copy to a removable drive, verify every file by hash, then eject safely.

### N14 — Navo doctor

Status: idea · Project: navo · Theme: workflow · Needs: —

One command that checks what slows Navo down (Defender scanning copies, long paths switched off, a stale index) and says what to do about each.

### N15 — The day's timeline

Status: idea · Project: navo · Theme: workflow · Needs: —

"Where was I yesterday at 15:00?": the folders visited, by hour.
