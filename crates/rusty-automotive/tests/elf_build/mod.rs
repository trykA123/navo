#![allow(
    dead_code,
    clippy::unwrap_used,
    reason = "test helpers; each test binary uses a part of them"
)]

use std::io::Cursor;
use std::sync::atomic::AtomicBool;

pub use rusty_automotive::elf_sample::*;
use rusty_automotive::{Elf, ElfReader, IndexError, Lines, Watch};

pub fn build(spec: &Spec) -> Vec<u8> {
    rusty_automotive::elf_sample::build(spec).unwrap()
}

pub fn line_program(
    is_64: bool,
    is_big: bool,
    rows: &[(u64, u64, u64)],
) -> Vec<(&'static str, Vec<u8>)> {
    rusty_automotive::elf_sample::line_program(is_64, is_big, rows).unwrap()
}

pub fn read(bytes: &[u8]) -> Result<(Elf, Lines), IndexError> {
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let reader = ElfReader::new(Cursor::new(bytes));
    let elf = reader.parse(&mut watch)?;
    let lines = reader.lines(&elf, &mut watch)?;
    Ok((elf, lines))
}

pub fn parse(bytes: &[u8]) -> Elf {
    read(bytes).unwrap().0
}

pub fn named<'a>(elf: &'a Elf, name: &str) -> Option<(usize, &'a rusty_automotive::Symbol)> {
    elf.symbols
        .iter()
        .enumerate()
        .find(|(_, symbol)| elf.name(symbol) == name)
}

pub fn line_program_in(directory: &[u8], rows: &[(u64, u64, u64)]) -> Vec<(&'static str, Vec<u8>)> {
    rusty_automotive::elf_sample::line_program_in(directory, (false, false), rows).unwrap()
}

pub fn rh850(text_at: u64, rows: &[(u64, u64, u64)]) -> Spec {
    Spec {
        is_64: false,
        is_big: false,
        machine: EM_V800,
        entry: text_at,
        parts: vec![text(text_at, 0x800)],
        symbols: Some(vec![
            func("_RESET", text_at, 0x10, 0),
            func("main", text_at + 0x400, 0x40, 0),
            func("tick", text_at + 0x480, 0x20, 0),
        ]),
        loads: vec![(0, 5)],
        debug: line_program(false, false, rows),
    }
}
