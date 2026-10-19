mod elf_build;

use elf_build::{EM_V800, build, firmware, line_program, named, parse, read};
use object::elf as raw;
use rusty_automotive::{Bind, Class, ElfProblem, Endian, IndexError, Place, SymbolKind, sniff_elf};

#[test]
fn the_magic_decides_not_the_extension() {
    let bytes = build(&firmware(false, false));
    assert!(sniff_elf(&bytes));
    assert!(!sniff_elf(b"MZ\x90\x00"));
    assert!(!sniff_elf(b"\x7FELF"), "class and data bytes are missing");
    assert!(!sniff_elf(b"hello world, this is a text .out file"));
}

#[test]
fn every_class_and_byte_order_reads_the_same_firmware() {
    for (is_64, is_big) in [(false, false), (false, true), (true, false), (true, true)] {
        let elf = parse(&build(&firmware(is_64, is_big)));
        let class = if is_64 { Class::Elf64 } else { Class::Elf32 };
        let endian = if is_big { Endian::Big } else { Endian::Little };
        assert_eq!((elf.header.class, elf.header.endian), (class, endian));
        assert_eq!(elf.header.entry, 0x0800_0101);
        assert_eq!(elf.machine_name(), "ARM");
        let (_, main) = named(&elf, "main").unwrap();
        assert_eq!(
            (main.value, main.size),
            (0x0800_0101, 0x40),
            "{is_64} {is_big}"
        );
        assert_eq!(elf.section_of(main).unwrap().name, ".text");
    }
}

#[test]
fn sections_carry_address_size_and_flags() {
    let elf = parse(&build(&firmware(false, true)));
    let names: Vec<&str> = elf
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "",
            ".text",
            ".data",
            ".bss",
            ".symtab",
            ".strtab",
            ".shstrtab"
        ]
    );
    let bss = &elf.sections[3];
    assert_eq!((bss.address, bss.size), (0x2000_0040, 0x200));
    assert_eq!(bss.flag_letters(), "WA");
    assert_eq!(bss.kind_name(), "NOBITS");
    assert_eq!(elf.sections[1].flag_letters(), "AX");
}

#[test]
fn load_segments_give_load_and_ram_totals() {
    let elf = parse(&build(&firmware(false, false)));
    assert_eq!(elf.load_segments().count(), 3);
    assert_eq!(elf.segments[0].flag_letters(), "R-X");
    assert!(!elf.totals.from_segments, "sections give the totals");
    assert_eq!(elf.totals.load, 0x400 + 0x40);
    assert_eq!(elf.totals.ram, 0x40 + 0x200);
}

#[test]
fn symbols_keep_type_bind_and_place_and_skip_file_entries() {
    let elf = parse(&build(&firmware(false, false)));
    assert_eq!(elf.symbols.len(), 6);
    let (_, state) = named(&elf, "state").unwrap();
    assert_eq!(
        (state.kind, state.bind, state.place),
        (SymbolKind::Object, Bind::Local, Place::Section(2))
    );
    assert!(named(&elf, "main.c").is_none());
}

#[test]
fn cpp_names_are_demangled_and_keep_the_raw_name() {
    let elf = parse(&build(&firmware(false, false)));
    let (_, add) = named(&elf, "add(int, int)").unwrap();
    assert_eq!(elf.raw_name(add), Some("_Z3addii"));
}

#[test]
fn a_stripped_file_says_so_and_has_no_symbols() {
    let mut spec = firmware(false, true);
    spec.symbols = None;
    let elf = parse(&build(&spec));
    assert!(elf.symbols.is_empty());
    assert!(elf.problems.contains(&ElfProblem::Stripped));
    assert!(elf.problems.contains(&ElfProblem::NoDwarf));
}

#[test]
fn dwarf_lines_name_the_source_of_a_function() {
    for (is_64, is_big) in [(false, false), (false, true), (true, true)] {
        let mut spec = firmware(is_64, is_big);
        spec.debug = line_program(
            is_64,
            is_big,
            &[(0x0800_0100, 0x40, 12), (0x0800_0140, 0x10, 40)],
        );
        let (elf, lines) = read(&build(&spec)).unwrap();
        assert!(elf.has_dwarf());
        assert!(!elf.problems.contains(&ElfProblem::NoDwarf));
        let (main, _) = named(&elf, "main").unwrap();
        assert_eq!(
            lines.get(main),
            Some(("/work/src/main.c", 12)),
            "{is_64} {is_big}"
        );
        let (reset, _) = named(&elf, "Reset_Handler").unwrap();
        assert_eq!(lines.get(reset), Some(("/work/src/main.c", 40)));
        let (counter, _) = named(&elf, "counter").unwrap();
        assert_eq!(lines.get(counter), None);
    }
}

#[test]
fn without_dwarf_no_symbol_has_a_line() {
    let (elf, lines) = read(&build(&firmware(false, false))).unwrap();
    assert!(elf.problems.contains(&ElfProblem::NoDwarf));
    assert_eq!(lines.found(), 0);
}

#[test]
fn renesas_v800_is_named() {
    let mut spec = firmware(false, false);
    spec.machine = EM_V800;
    assert_eq!(parse(&build(&spec)).machine_name(), "Renesas V800 / RH850");
}

#[test]
fn a_truncated_header_is_an_error_not_a_panic() {
    let bytes = build(&firmware(false, false));
    let error = read(&bytes[..20]).unwrap_err();
    assert!(matches!(error, IndexError::Malformed(_)), "{error:?}");
}

#[test]
fn a_section_table_past_the_end_is_a_problem() {
    let mut bytes = build(&firmware(false, false));
    let shoff = 0x20;
    bytes[shoff..shoff + 4].copy_from_slice(&0x7FFF_FFF0u32.to_le_bytes());
    let elf = parse(&bytes);
    assert!(elf.sections.is_empty());
    assert!(
        elf.problems
            .iter()
            .any(|problem| matches!(problem, ElfProblem::Malformed(text) if text.contains("section headers"))),
        "{:?}",
        elf.problems
    );
    assert_eq!(elf.load_segments().count(), 3);
}

#[test]
fn a_symbol_table_with_a_bogus_size_is_a_problem() {
    let spec = firmware(false, false);
    let mut bytes = build(&spec);
    let elf = parse(&bytes);
    let symtab = elf
        .sections
        .iter()
        .position(|section| section.kind == raw::SHT_SYMTAB)
        .unwrap();
    let shoff = u32::from_le_bytes(bytes[0x20..0x24].try_into().unwrap()) as usize;
    let size_at = shoff + symtab * 40 + 20;
    bytes[size_at..size_at + 4].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    let elf = parse(&bytes);
    assert!(elf.symbols.is_empty());
    assert!(
        elf.problems
            .iter()
            .any(|problem| matches!(problem, ElfProblem::Malformed(_))),
        "{:?}",
        elf.problems
    );
}
