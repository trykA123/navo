//! Builds ELF fixtures with the low-level object writer for automotive samples and tests.

use object::write::StringId;
use object::write::elf::{FileHeader, ProgramHeader, SectionHeader, SectionIndex, Sym, Writer};
use object::{Endianness, elf};

use super::Spec;

struct Reserved {
    parts: Vec<(StringId, SectionIndex)>,
    debug: Vec<(StringId, SectionIndex)>,
    names: Vec<StringId>,
    offsets: Vec<u64>,
    debug_offsets: Vec<u64>,
}

/// Reserves symbol names and indices for the supplied fixture sections.
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

/// Reserves fixture offsets in bytes, failing on excessive segment counts or invalid tables.
fn reserve<'a>(writer: &mut Writer<'a>, spec: &'a Spec) -> Result<Reserved, String> {
    writer.reserve_file_header();
    let segments = u32::try_from(spec.loads.len()).map_err(|error| error.to_string())?;
    writer.reserve_program_headers(segments);
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
        .map(|part| writer.reserve(part.data.len() as u64, 4))
        .collect();
    let debug_offsets = spec
        .debug
        .iter()
        .map(|(_, bytes)| writer.reserve(bytes.len() as u64, 1))
        .collect();
    reserve_tables(writer, spec)?;
    Ok(Reserved {
        parts,
        debug,
        names,
        offsets,
        debug_offsets,
    })
}

/// Reserves string and section tables, returning errors for invalid names or oversized tables.
fn reserve_tables(writer: &mut Writer<'_>, spec: &Spec) -> Result<(), String> {
    if spec.symbols.is_some() {
        writer.reserve_symtab();
        writer.reserve_strtab().map_err(|error| error.to_string())?;
    }
    writer
        .reserve_shstrtab()
        .map_err(|error| error.to_string())?;
    writer.reserve_section_headers();
    Ok(())
}

/// Writes reserved fixture symbols and their string table.
fn write_symbols(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
    if let Some(symbols) = &spec.symbols {
        writer.write_null_symbol();
        for (symbol, name) in symbols.iter().zip(&reserved.names) {
            writer.write_symbol(&Sym {
                st_name: writer.string_offset(Some(*name)),
                section: symbol
                    .part
                    .and_then(|at| reserved.parts.get(at))
                    .map(|part| part.1.0),
                st_info: elf::SymbolInfo((symbol.bind << 4) | symbol.kind),
                st_other: elf::SymbolOther(0),
                st_shndx: if symbol.is_absolute {
                    elf::SHN_ABS
                } else {
                    elf::SHN_UNDEF
                },
                st_value: symbol.value,
                st_size: symbol.size,
            });
        }
        writer.write_strtab();
    }
}

/// Writes fixture contents, failing on invalid headers or missing segment sections.
fn write_contents(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) -> Result<(), String> {
    writer
        .write_file_header(&FileHeader {
            os_abi: elf::ELFOSABI_NONE,
            abi_version: 0,
            e_type: elf::ET_EXEC,
            e_machine: elf::Machine(spec.machine),
            e_entry: spec.entry,
            e_flags: elf::FileFlags(0),
        })
        .map_err(|error| error.to_string())?;
    writer.write_align_program_headers();
    for (at, flags) in &spec.loads {
        let (Some(part), Some(offset)) = (spec.parts.get(*at), reserved.offsets.get(*at)) else {
            return Err(format!("load segment {at} names no section"));
        };
        writer.write_program_header(&ProgramHeader {
            p_type: elf::PT_LOAD,
            p_flags: elf::ProgramFlags(*flags),
            p_offset: *offset,
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

/// Builds a section header from its name offset, numeric flags, and byte range.
fn header(
    sh_name: u32,
    (sh_type, sh_flags): (u32, u64),
    (sh_addr, sh_offset, sh_size): (u64, u64, u64),
) -> SectionHeader {
    SectionHeader {
        sh_name,
        sh_type: elf::SectionType(sh_type),
        sh_flags: elf::SectionFlags(sh_flags),
        sh_addr,
        sh_offset,
        sh_size,
        sh_link: 0,
        sh_info: 0,
        sh_addralign: 1,
        sh_entsize: 0,
    }
}

/// Writes fixture section headers with the reserved string table offsets.
fn write_headers(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
    writer.write_null_section_header();
    for ((part, (name, _)), offset) in spec
        .parts
        .iter()
        .zip(&reserved.parts)
        .zip(&reserved.offsets)
    {
        writer.write_section_header(&header(
            writer.section_name_offset(Some(*name)),
            (part.kind, part.flags),
            (part.address, *offset, part.size),
        ));
    }
    for (((_, bytes), (name, _)), offset) in spec
        .debug
        .iter()
        .zip(&reserved.debug)
        .zip(&reserved.debug_offsets)
    {
        writer.write_section_header(&header(
            writer.section_name_offset(Some(*name)),
            (elf::SHT_PROGBITS.0, 0),
            (0, *offset, bytes.len() as u64),
        ));
    }
    if spec.symbols.is_some() {
        writer.write_symtab_section_header(1);
        writer.write_strtab_section_header();
    }
    writer.write_shstrtab_section_header();
}

/// Builds ELF bytes from a fixture specification, returning header or string table errors.
pub fn build(spec: &Spec) -> Result<Vec<u8>, String> {
    let endian = if spec.is_big {
        Endianness::Big
    } else {
        Endianness::Little
    };
    let mut buffer = Vec::new();
    let mut writer = Writer::new(endian, spec.is_64, &mut buffer);
    let reserved = reserve(&mut writer, spec)?;
    write_contents(&mut writer, spec, &reserved)?;
    write_headers(&mut writer, spec, &reserved);
    Ok(buffer)
}
