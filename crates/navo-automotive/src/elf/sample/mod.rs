//! Builds sample firmware specifications and DWARF line programs for automotive fixtures.

use gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, LineProgram, LineString, Sections,
};
use gimli::{Encoding, Format, LineEncoding, RunTimeEndian};
use object::elf;

mod demo;
mod writer;

pub use demo::demo;
pub use writer::build;

pub const EM_V800: u16 = 36;

#[derive(Clone)]
pub struct Part {
    pub name: &'static str,
    pub kind: u32,
    pub flags: u64,
    pub address: u64,
    pub load_address: u64,
    pub data: Vec<u8>,
    pub size: u64,
}

#[derive(Clone)]
pub struct Named {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub kind: u8,
    pub bind: u8,
    pub part: Option<usize>,
    pub is_absolute: bool,
}

#[derive(Clone)]
pub struct Spec {
    pub is_64: bool,
    pub is_big: bool,
    pub machine: u16,
    pub entry: u64,
    pub parts: Vec<Part>,
    pub symbols: Option<Vec<Named>>,
    pub loads: Vec<(usize, u32)>,
    pub debug: Vec<(&'static str, Vec<u8>)>,
}

/// Creates an executable section with a byte address and byte length.
pub fn text(address: u64, len: usize) -> Part {
    Part {
        name: ".text",
        kind: elf::SHT_PROGBITS.0,
        flags: elf::SHF_ALLOC.0 | elf::SHF_EXECINSTR.0,
        address,
        load_address: address,
        data: vec![0x90; len],
        size: len as u64,
    }
}

/// Creates a writable section with a byte address and byte length.
pub fn data(address: u64, len: usize) -> Part {
    Part {
        name: ".data",
        kind: elf::SHT_PROGBITS.0,
        flags: elf::SHF_ALLOC.0 | elf::SHF_WRITE.0,
        address,
        load_address: address,
        data: vec![0x11; len],
        size: len as u64,
    }
}

/// Creates an uninitialized writable section with a byte address and byte size.
pub fn bss(address: u64, size: u64) -> Part {
    Part {
        name: ".bss",
        kind: elf::SHT_NOBITS.0,
        flags: elf::SHF_ALLOC.0 | elf::SHF_WRITE.0,
        address,
        load_address: address,
        data: Vec::new(),
        size,
    }
}

/// Creates a global function symbol with a byte address, byte size, and section index.
pub fn func(name: &str, value: u64, size: u64, part: usize) -> Named {
    Named {
        name: name.to_owned(),
        value,
        size,
        kind: elf::STT_FUNC.0,
        bind: elf::STB_GLOBAL.0,
        part: Some(part),
        is_absolute: false,
    }
}

/// Creates a global data symbol with a byte address, byte size, and section index.
pub fn object(name: &str, value: u64, size: u64, part: usize) -> Named {
    Named {
        kind: elf::STT_OBJECT.0,
        ..func(name, value, size, part)
    }
}

/// Creates an absolute symbol with a name and byte value.
pub fn absolute(name: &str, value: u64) -> Named {
    Named {
        kind: elf::STT_NOTYPE.0,
        part: None,
        is_absolute: true,
        ..func(name, value, 0, 0)
    }
}

/// Creates sample firmware for the requested ELF class and endianness.
pub fn firmware(is_64: bool, is_big: bool) -> Spec {
    Spec {
        is_64,
        is_big,
        machine: elf::EM_ARM.0,
        entry: 0x0800_0101,
        parts: vec![
            text(0x0800_0000, 0x400),
            Part {
                load_address: 0x0800_0400,
                ..data(0x2000_0000, 0x40)
            },
            bss(0x2000_0040, 0x200),
        ],
        symbols: Some(vec![
            func("main", 0x0800_0101, 0x40, 0),
            func("Reset_Handler", 0x0800_0141, 0x10, 0),
            func("_Z3addii", 0x0800_0151, 0x8, 0),
            object("counter", 0x2000_0000, 4, 1),
            object("rx_buffer", 0x2000_0040, 0x100, 2),
            Named {
                bind: elf::STB_LOCAL.0,
                ..object("state", 0x2000_0004, 1, 1)
            },
            Named {
                kind: elf::STT_FILE.0,
                bind: elf::STB_LOCAL.0,
                part: None,
                ..func("main.c", 0, 0, 0)
            },
        ]),
        loads: vec![
            (0, elf::PF_R.0 | elf::PF_X.0),
            (1, elf::PF_R.0 | elf::PF_W.0),
            (2, elf::PF_R.0 | elf::PF_W.0),
        ],
        debug: Vec::new(),
    }
}

pub type DebugSections = Vec<(&'static str, Vec<u8>)>;

pub fn line_program(
    is_64: bool,
    is_big: bool,
    rows: &[(u64, u64, u64)],
) -> Result<DebugSections, String> {
    line_program_in(b"/work/src", (is_64, is_big), rows)
}

pub fn line_program_in(
    directory: &[u8],
    (is_64, is_big): (bool, bool),
    rows: &[(u64, u64, u64)],
) -> Result<DebugSections, String> {
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: if is_64 { 8 } else { 4 },
    };
    let mut dwarf = DwarfUnit::new(encoding);
    let root = dwarf.unit.root();
    dwarf.unit.get_mut(root).set(
        gimli::DW_AT_comp_dir,
        AttributeValue::String(b"/work".to_vec()),
    );
    let mut program = LineProgram::new(
        encoding,
        LineEncoding::default(),
        LineString::String(b"/work".to_vec()),
        None,
        LineString::String(b"main.c".to_vec()),
        None,
    );
    let directory = program.add_directory(LineString::String(directory.to_vec()));
    let file = program.add_file(LineString::String(b"main.c".to_vec()), directory, None);
    for (start, len, line) in rows {
        program.begin_sequence(Some(Address::Constant(*start)));
        program.row().file = file;
        program.row().line = *line;
        program.generate_row();
        program.end_sequence(*len);
    }
    dwarf.unit.line_program = program;
    let endian = if is_big {
        RunTimeEndian::Big
    } else {
        RunTimeEndian::Little
    };
    let mut sections = Sections::new(EndianVec::new(endian));
    dwarf
        .write(&mut sections)
        .map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    sections
        .for_each(|id, data| {
            if !data.slice().is_empty() {
                out.push((id.name(), data.slice().to_vec()));
            }
            Ok::<_, gimli::write::Error>(())
        })
        .map_err(|error| error.to_string())?;
    Ok(out)
}
