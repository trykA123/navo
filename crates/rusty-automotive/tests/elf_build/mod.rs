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
