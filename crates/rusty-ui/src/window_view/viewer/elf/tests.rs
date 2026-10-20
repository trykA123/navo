use rusty_automotive::elf_sample::{build, firmware};
use rusty_automotive::{Elf, ElfReader, Watch};

use super::text::{Spot, address, parse_address, row_label, source, span, status};

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
    let texts: Vec<String> = status(&elf, None, 0)
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
    assert_eq!(
        source(&elf, None, Spot::Symbol(0)),
        "No DWARF line info in this file"
    );
}

#[test]
fn addresses_parse_with_or_without_a_prefix() {
    assert_eq!(parse_address("0x8000400"), Ok(0x800_0400));
    assert_eq!(parse_address(" 1F00 "), Ok(0x1F00));
    assert!(parse_address("zz").is_err());
}

#[test]
fn a_long_symbol_keeps_its_prefix_and_both_ends() {
    let fitted = super::columns::fitted_name("Can_MainFunction_ReadAllChannelsAndReport", 24);
    assert_eq!(fitted.prefix, "Can_");
    assert!(fitted.rest.contains('…'));
    assert!(fitted.rest.starts_with("Main") && fitted.rest.ends_with("Report"));
    assert!(fitted.prefix.chars().count() + fitted.rest.chars().count() <= 24);
}

#[test]
fn a_short_symbol_stays_whole() {
    let fitted = super::columns::fitted_name("Can_Init", 24);
    assert_eq!(
        (fitted.prefix.as_str(), fitted.rest.as_str()),
        ("Can_", "Init")
    );
}
