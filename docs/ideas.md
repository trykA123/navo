# Navo ideas

A backlog of accepted ideas that are not in a plan yet. The owner accepted every idea below on 2026-11-08. Each becomes a plan when its turn comes in the [roadmap](roadmap.md).

## The automotive module (D112)

After the module registry ships (roadmap v1.0, item 5), everything automotive and embedded moves into one module, which users can switch off. Not every user wants it.

**Moves into the module:**
- the ELF and MAP viewers, simple and rich ([rich-elf-map](features/rich-elf-map.md));
- HEX and S-record;
- CAN traces with DBC, MDF4, A2L;
- the m4, OIL and DCM colouring.

**New in the module:**
1. **Memory delta between two builds:** two MAP or ELF files side by side. It shows which module grew or shrank in flash and RAM, and by how many bytes. An optional budget warns, for example "RAM above 90%". It reuses the compare work of [compare](features/compare.md) where it fits.
2. **HEX and S-record compare by address:** the blocks that differ between two images, and a CRC over an address range, as a bootloader computes it.
3. **Symbols in Ctrl+K:** type a function or variable name and get its address, size and build, across build folders.
4. **Address to source:** paste an address from a crash or trap and get the function and `file:line` from DWARF, like `addr2line`.
5. **CAN signal chart:** on top of the DBC decoding, pick 2–3 signals and chart them over time, with keyboard zoom.

**Decide:** the module's name. Proposed: "Automotive", with "embedded" in its description.

## Core and other modules

6. **Key trainer:** Navo notices, locally, actions done by mouse or menu that have a key, and says so once: "Copy from the menu 12 times; F5 does it." It can be switched off.
7. **Local usage statistics (opt-in):** which commands the owner uses every day and which features were never touched, so the roadmap is ordered by real use. Nothing leaves the machine.
8. **Folder notes:** a small Markdown note attached to a folder, for example "this is the release candidate for X", shown in the drawer.
9. **Huge-file mode:** open a log or CAN trace of several GB at once: memory-mapped, line offsets indexed, jump to a time, search inside.

## Learning

10. **A Navo track in Cockpit's Study tab:** Navo's own code as Rust and algorithm lessons: the crash-safe journal (a write-ahead log), the file index layout (2 million entries in about 97 MiB), Ctrl+K fuzzy scoring, shared listings (`Arc` and copy-on-write), the NTFS USN journal, and mimalloc (allocators). Tracked in Cockpit's `plans/2026-11-09-cockpit-next.md` (I13, I15).
