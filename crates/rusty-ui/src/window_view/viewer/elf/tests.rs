use rusty_automotive::elf_sample::{build, firmware};
use rusty_automotive::{Elf, ElfReader, Watch};

use super::text::{address, row_label, source, span, status};

fn parse(is_64: bool) -> Elf {
    let bytes = build(&firmware(is_64, false)).expect("the sample builds");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    ElfReader::new(std::io::Cursor::new(bytes))
        .parse(&mut watch)
        .expect("the sample parses")
}

#[test]
fn addresses_use_the_width_of_the_class() {
    assert_eq!(address(0x800_0101, false), "0x08000101");
    assert_eq!(address(0x800_0101, true), "0x0000000008000101");
}

#[test]
fn a_span_names_its_last_byte() {
    assert_eq!(span(0x100, 0x40, false), "0x00000100–0x0000013F");
    assert_eq!(span(0x100, 0, false), "0x00000100");
}

#[test]
fn a_row_reads_as_its_columns() {
    let elf = parse(false);
    let main = elf
        .symbols
        .iter()
        .find(|symbol| elf.name(symbol) == "main")
        .expect("main");
    assert_eq!(
        row_label(&elf, main),
        "main, 0x08000101, 64, func, global, .text"
    );
}

#[test]
fn the_status_names_machine_class_entry_totals_and_problems() {
    let elf = parse(true);
    let texts: Vec<String> = status(&elf, None)
        .into_iter()
        .map(|check| check.text)
        .collect();
    assert_eq!(
        texts,
        [
            "ARM",
            "ELF64 little-endian",
            "entry 0x0000000008000101",
            "load 1.1 KB · RAM 576 B",
            "6 symbols",
            "no DWARF line info",
        ]
    );
}

#[test]
fn without_dwarf_the_source_says_so() {
    let elf = parse(false);
    assert_eq!(source(&elf, None, 0), "No DWARF line info in this file");
}
