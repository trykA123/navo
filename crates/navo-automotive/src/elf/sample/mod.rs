use gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, LineProgram, LineString, Sections,
};
use gimli::{Encoding, Format, LineEncoding, RunTimeEndian};
use object::Endianness;
use object::elf;
use object::write::StringId;
use object::write::elf::{FileHeader, ProgramHeader, SectionHeader, SectionIndex, Sym, Writer};

mod demo;

pub use demo::demo;

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

pub fn text(address: u64, len: usize) -> Part {
    Part {
        name: ".text",
        kind: elf::SHT_PROGBITS,
        flags: u64::from(elf::SHF_ALLOC | elf::SHF_EXECINSTR),
        address,
        load_address: address,
        data: vec![0x90; len],
        size: len as u64,
    }
}

pub fn data(address: u64, len: usize) -> Part {
    Part {
        name: ".data",
        kind: elf::SHT_PROGBITS,
        flags: u64::from(elf::SHF_ALLOC | elf::SHF_WRITE),
        address,
        load_address: address,
        data: vec![0x11; len],
        size: len as u64,
    }
}

pub fn bss(address: u64, size: u64) -> Part {
    Part {
        name: ".bss",
        kind: elf::SHT_NOBITS,
        flags: u64::from(elf::SHF_ALLOC | elf::SHF_WRITE),
        address,
        load_address: address,
        data: Vec::new(),
        size,
    }
}

pub fn func(name: &str, value: u64, size: u64, part: usize) -> Named {
    Named {
        name: name.to_owned(),
        value,
        size,
        kind: elf::STT_FUNC,
        bind: elf::STB_GLOBAL,
        part: Some(part),
        is_absolute: false,
    }
}

pub fn object(name: &str, value: u64, size: u64, part: usize) -> Named {
    Named {
        kind: elf::STT_OBJECT,
        ..func(name, value, size, part)
    }
}

pub fn absolute(name: &str, value: u64) -> Named {
    Named {
        kind: elf::STT_NOTYPE,
        part: None,
        is_absolute: true,
        ..func(name, value, 0, 0)
    }
}

pub fn firmware(is_64: bool, is_big: bool) -> Spec {
    Spec {
        is_64,
        is_big,
        machine: elf::EM_ARM,
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
                bind: elf::STB_LOCAL,
                ..object("state", 0x2000_0004, 1, 1)
            },
            Named {
                kind: elf::STT_FILE,
                bind: elf::STB_LOCAL,
                part: None,
                ..func("main.c", 0, 0, 0)
            },
        ]),
        loads: vec![
            (0, elf::PF_R | elf::PF_X),
            (1, elf::PF_R | elf::PF_W),
            (2, elf::PF_R | elf::PF_W),
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

struct Reserved {
    parts: Vec<(StringId, SectionIndex)>,
    debug: Vec<(StringId, SectionIndex)>,
    names: Vec<StringId>,
    offsets: Vec<usize>,
    debug_offsets: Vec<usize>,
}

fn reserve_symbols<'a>(
    writer: &mut Writer<'a>,
    spec: &'a Spec,
    parts: &[(StringId, SectionIndex)],
) -> Vec<StringId> {
    let mut names = Vec::new();
    if let Some(symbols) = &spec.symbols {
        writer.reserve_null_symbol_index();
        for symbol in symbols {
            let section = symbol.part.and_then(|at| parts.get(at)).map(|part| part.1);
            names.push(writer.add_string(symbol.name.as_bytes()));
            writer.reserve_symbol_index(section);
        }
        writer.reserve_symtab_section_index();
        writer.reserve_strtab_section_index();
    }
    names
}

fn reserve<'a>(writer: &mut Writer<'a>, spec: &'a Spec) -> Reserved {
    writer.reserve_file_header();
    writer.reserve_program_headers(spec.loads.len() as u32);
    writer.reserve_null_section_index();
    let parts: Vec<_> = spec
        .parts
        .iter()
        .map(|part| {
            let name = writer.add_section_name(part.name.as_bytes());
            (name, writer.reserve_section_index())
        })
        .collect();
    let debug: Vec<_> = spec
        .debug
        .iter()
        .map(|(name, _)| {
            let name = writer.add_section_name(name.as_bytes());
            (name, writer.reserve_section_index())
        })
        .collect();
    let names = reserve_symbols(writer, spec, &parts);
    writer.reserve_shstrtab_section_index();
    let offsets = spec
        .parts
        .iter()
        .map(|part| writer.reserve(part.data.len(), 4))
        .collect();
    let debug_offsets = spec
        .debug
        .iter()
        .map(|(_, bytes)| writer.reserve(bytes.len(), 1))
        .collect();
    if spec.symbols.is_some() {
        writer.reserve_symtab();
        writer.reserve_strtab();
    }
    writer.reserve_shstrtab();
    writer.reserve_section_headers();
    Reserved {
        parts,
        debug,
        names,
        offsets,
        debug_offsets,
    }
}

fn write_symbols(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
    if let Some(symbols) = &spec.symbols {
        writer.write_null_symbol();
        for (symbol, name) in symbols.iter().zip(&reserved.names) {
            writer.write_symbol(&Sym {
                name: Some(*name),
                section: symbol
                    .part
                    .and_then(|at| reserved.parts.get(at))
                    .map(|part| part.1),
                st_info: (symbol.bind << 4) | symbol.kind,
                st_other: 0,
                st_shndx: if symbol.is_absolute { elf::SHN_ABS } else { 0 },
                st_value: symbol.value,
                st_size: symbol.size,
            });
        }
        writer.write_strtab();
    }
}

fn write_contents(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) -> Result<(), String> {
    writer
        .write_file_header(&FileHeader {
            os_abi: 0,
            abi_version: 0,
            e_type: elf::ET_EXEC,
            e_machine: spec.machine,
            e_entry: spec.entry,
            e_flags: 0,
        })
        .map_err(|error| error.to_string())?;
    writer.write_align_program_headers();
    for (at, flags) in &spec.loads {
        let (Some(part), Some(offset)) = (spec.parts.get(*at), reserved.offsets.get(*at)) else {
            return Err(format!("load segment {at} names no section"));
        };
        writer.write_program_header(&ProgramHeader {
            p_type: elf::PT_LOAD,
            p_flags: *flags,
            p_offset: *offset as u64,
            p_vaddr: part.address,
            p_paddr: part.load_address,
            p_filesz: part.data.len() as u64,
            p_memsz: part.size,
            p_align: 4,
        });
    }
    for part in &spec.parts {
        writer.write_align(4);
        writer.write(&part.data);
    }
    for (_, bytes) in &spec.debug {
        writer.write(bytes);
    }
    write_symbols(writer, spec, reserved);
    writer.write_shstrtab();
    Ok(())
}

fn header(
    name: StringId,
    (sh_type, sh_flags): (u32, u64),
    (sh_addr, sh_offset, sh_size): (u64, u64, u64),
) -> SectionHeader {
    SectionHeader {
        name: Some(name),
        sh_type,
        sh_flags,
        sh_addr,
        sh_offset,
        sh_size,
        sh_link: 0,
        sh_info: 0,
        sh_addralign: 1,
        sh_entsize: 0,
    }
}

fn write_headers(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
    writer.write_null_section_header();
    for ((part, (name, _)), offset) in spec
        .parts
        .iter()
        .zip(&reserved.parts)
        .zip(&reserved.offsets)
    {
        writer.write_section_header(&header(
            *name,
            (part.kind, part.flags),
            (part.address, *offset as u64, part.size),
        ));
    }
    for (((_, bytes), (name, _)), offset) in spec
        .debug
        .iter()
        .zip(&reserved.debug)
        .zip(&reserved.debug_offsets)
    {
        writer.write_section_header(&header(
            *name,
            (elf::SHT_PROGBITS, 0),
            (0, *offset as u64, bytes.len() as u64),
        ));
    }
    if spec.symbols.is_some() {
        writer.write_symtab_section_header(1);
        writer.write_strtab_section_header();
    }
    writer.write_shstrtab_section_header();
}

pub fn build(spec: &Spec) -> Result<Vec<u8>, String> {
    let endian = if spec.is_big {
        Endianness::Big
    } else {
        Endianness::Little
    };
    let mut buffer = Vec::new();
    let mut writer = Writer::new(endian, spec.is_64, &mut buffer);
    let reserved = reserve(&mut writer, spec);
    write_contents(&mut writer, spec, &reserved)?;
    write_headers(&mut writer, spec, &reserved);
    Ok(buffer)
}
