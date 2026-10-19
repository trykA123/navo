mod elf_build;

use elf_build::{build, line_program_in, named, read, rh850};
use rusty_automotive::ElfProblem;

#[allow(clippy::unwrap_used, reason = "test helper")]
fn line_of(spec: &elf_build::Spec, name: &str) -> Option<(String, u32)> {
    let (elf, lines) = read(&build(spec)).unwrap();
    let (at, _) = named(&elf, name).unwrap();
    lines.get(at).map(|(file, line)| (file.to_owned(), line))
}

#[test]
fn discarded_sequences_at_zero_do_not_name_a_function_at_zero() {
    let spec = rh850(0, &[(0, 0x40, 99), (0, 0x20, 98), (0x400, 0x40, 12)]);
    assert_eq!(line_of(&spec, "_RESET"), None);
    assert_eq!(
        line_of(&spec, "main"),
        Some(("/work/src/main.c".to_owned(), 12))
    );
}

#[test]
fn a_single_sequence_at_zero_is_real_code() {
    let spec = rh850(0, &[(0, 0x40, 7), (0x400, 0x40, 12)]);
    assert_eq!(
        line_of(&spec, "_RESET"),
        Some(("/work/src/main.c".to_owned(), 7))
    );
}

#[test]
fn sequences_outside_code_never_match() {
    let spec = rh850(0x400, &[(0, 0x1000, 99), (0x800, 0x40, 12)]);
    assert_eq!(line_of(&spec, "_RESET"), None);
    assert_eq!(
        line_of(&spec, "main"),
        Some(("/work/src/main.c".to_owned(), 12))
    );
}

#[test]
fn a_sequence_starting_at_the_symbol_wins_over_one_that_covers_it() {
    let spec = rh850(0, &[(0x400, 0x100, 50), (0x480, 0x20, 80)]);
    assert_eq!(
        line_of(&spec, "tick"),
        Some(("/work/src/main.c".to_owned(), 80))
    );
}

#[test]
fn any_address_inside_code_has_a_line() {
    let spec = rh850(0, &[(0x400, 0x40, 12)]);
    let (_, lines) = read(&build(&spec)).unwrap();
    assert_eq!(lines.at(0x420), Some(("/work/src/main.c", 12)));
    assert_eq!(lines.at(0x440), None);
}

#[test]
fn a_relative_include_directory_joins_the_compilation_directory() {
    let mut spec = rh850(0, &[(0x400, 0x40, 12)]);
    spec.debug = line_program_in(b"src", &[(0x400, 0x40, 12)]);
    assert_eq!(
        line_of(&spec, "main"),
        Some(("/work/src/main.c".to_owned(), 12))
    );
}

#[test]
fn a_debug_section_outside_the_file_is_a_problem() {
    let spec = rh850(0, &[(0x400, 0x40, 12)]);
    let mut bytes = build(&spec);
    let (elf, _) = read(&bytes).unwrap();
    let line = elf
        .sections
        .iter()
        .position(|section| section.name == ".debug_line")
        .unwrap();
    let shoff = u32::from_le_bytes(bytes[0x20..0x24].try_into().unwrap()) as usize;
    let offset_at = shoff + line * 40 + 16;
    bytes[offset_at..offset_at + 4].copy_from_slice(&0x7FFF_0000u32.to_le_bytes());
    let (_, lines) = read(&bytes).unwrap();
    assert!(
        lines.problems.contains(&ElfProblem::Malformed(
            ".debug_line lies outside the file".to_owned()
        )),
        "{:?}",
        lines.problems
    );
}
