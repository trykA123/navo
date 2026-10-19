use rusty_automotive::{Class, Elf, Endian, Lines, Place, Symbol};

use crate::format;

pub(super) fn address(at: u64, wide: bool) -> String {
    if wide {
        format!("0x{at:016X}")
    } else {
        format!("0x{at:08X}")
    }
}

pub(super) fn is_wide(elf: &Elf) -> bool {
    elf.header.class == Class::Elf64
}

pub(super) fn place(elf: &Elf, symbol: &Symbol) -> String {
    match symbol.place {
        Place::Section(_) => elf
            .section_of(symbol)
            .map_or_else(String::new, |section| section.name.clone()),
        Place::Undefined => "UNDEF".to_owned(),
        Place::Absolute => "ABS".to_owned(),
        Place::Common => "COMMON".to_owned(),
        Place::Other(index) => format!("#{index}"),
    }
}

pub(super) fn row_label(elf: &Elf, symbol: &Symbol) -> String {
    format!(
        "{}, {}, {}, {}, {}, {}",
        elf.name(symbol),
        address(symbol.value, is_wide(elf)),
        format::bytes(symbol.size),
        symbol.kind.name(),
        symbol.bind.name(),
        place(elf, symbol)
    )
}

pub(super) fn class(elf: &Elf) -> String {
    let class = match elf.header.class {
        Class::Elf32 => "ELF32",
        Class::Elf64 => "ELF64",
    };
    let endian = match elf.header.endian {
        Endian::Little => "little-endian",
        Endian::Big => "big-endian",
    };
    format!("{class} {endian}")
}

pub(super) struct Check {
    pub text: String,
    pub is_bad: bool,
}

fn check(text: String, is_bad: bool) -> Check {
    Check { text, is_bad }
}

pub(super) fn status(elf: &Elf, lines: Option<&Lines>) -> Vec<Check> {
    let wide = is_wide(elf);
    let totals = elf.totals;
    let mut checks = vec![
        check(elf.machine_name(), false),
        check(class(elf), false),
        check(format!("entry {}", address(elf.header.entry, wide)), false),
        check(
            format!(
                "load {} · RAM {}",
                format::size(totals.load),
                format::size(totals.ram)
            ),
            false,
        ),
        check(
            format::count(elf.symbols.len() as u64, "symbol", "symbols"),
            false,
        ),
    ];
    checks.extend(
        elf.problems
            .iter()
            .map(|problem| check(problem.to_string(), true)),
    );
    if let Some(lines) = lines {
        checks.extend(
            lines
                .problems
                .iter()
                .map(|problem| check(problem.to_string(), true)),
        );
    }
    checks
}

pub(super) fn parse_address(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).map_err(|_| format!("Not a hex address: {trimmed}"))
}

pub(super) enum Spot {
    Symbol(usize),
    Address(u64),
}

pub(super) fn source(elf: &Elf, lines: Option<&Lines>, spot: Spot) -> String {
    if !elf.has_dwarf() {
        return "No DWARF line info in this file".to_owned();
    }
    let Some(lines) = lines else {
        return "Reading DWARF lines…".to_owned();
    };
    let found = match spot {
        Spot::Symbol(at) => lines.get(at),
        Spot::Address(at) => lines.at(at),
    };
    match found {
        Some((file, 0)) => file.to_owned(),
        Some((file, line)) => format!("{file}:{line}"),
        None => "No source line for this address".to_owned(),
    }
}

pub(super) fn span(start: u64, size: u64, wide: bool) -> String {
    if size == 0 {
        return address(start, wide);
    }
    format!(
        "{}–{}",
        address(start, wide),
        address(start.saturating_add(size - 1), wide)
    )
}
