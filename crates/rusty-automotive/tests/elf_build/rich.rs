use std::fmt::Write;

use object::elf;

use super::elf_build::{Named, Part, Spec, absolute, bss, data, func, line_program, object, text};

pub const ROM: u64 = 0x0800_0000;
const RAM: u64 = 0x2000_0000;
const ROM_SIZE: u64 = 0x0100_0000;
const RAM_SIZE: u64 = 0x0020_0000;
pub const MDF: &str = "space linear 1 8\nchip flash 1 8 0x1000000 rom\nchip ram 2 8 0x200000 ram\nmap 1 0x08000000 0x1000000 1 0\nmap 1 0x20000000 0x200000 2 0\n";
const MODULES: [&str; 40] = [
    "Com", "Lamp", "Seat", "Horn", "Door", "Fan", "Mirror", "Clock", "Gauge", "Wheel", "Brake",
    "Motor", "Pump", "Valve", "Heater", "Cooler", "Wiper", "Radio", "Panel", "Sensor", "Relay",
    "Switch", "Latch", "Window", "Roof", "Trunk", "Pedal", "Beacon", "Buzzer", "Timer", "Drive",
    "Charge", "Battery", "Cable", "Fuse", "Light", "Signal", "Alarm", "Lock", "Display",
];

pub struct Fixture {
    pub spec: Spec,
    names: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            spec: Spec {
                is_64: false,
                is_big: false,
                machine: super::elf_build::EM_V800,
                entry: ROM,
                parts: Vec::new(),
                symbols: Some(Vec::new()),
                loads: Vec::new(),
                debug: Vec::new(),
            },
            names: Vec::new(),
        }
    }

    fn push(&mut self, name: String, part: Part) -> usize {
        let at = self.spec.parts.len();
        if part.flags & u64::from(elf::SHF_ALLOC) != 0 {
            let flags = if part.flags & u64::from(elf::SHF_EXECINSTR) != 0 {
                5
            } else {
                6
            };
            self.spec.loads.push((at, flags));
        }
        self.spec.parts.push(part);
        self.names.push(name);
        at
    }

    pub fn build(mut self) -> Vec<u8> {
        if let Some(symbols) = &mut self.spec.symbols {
            symbols.sort_by_key(|symbol| symbol.bind != elf::STB_LOCAL);
        }
        writer::build(&self.spec, &self.names)
    }

    pub fn map(&self) -> String {
        let mut result = String::from(
            "TASKING VX\n* Link Result\n| File | Section | Size | Offset | Section | Size |\n",
        );
        for (at, (part, name)) in self.spec.parts.iter().zip(&self.names).enumerate() {
            if part.flags & u64::from(elf::SHF_ALLOC) == 0 || name.starts_with("$fill") {
                continue;
            }
            let module = MODULES[at % MODULES.len()];
            writeln!(result, "| obj/{module}_Unit{at}.o | {name} ({at}) | 0x{:x} | 0x0 | {name} ({at}) | 0x{:x} |", part.size, part.size).unwrap();
        }
        result.push_str("* Locate Result\n* Sections\n+ Space chip:core:linear (MAU = 8bit)\n| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |\n");
        let mut used = [0u64; 2];
        for (at, (part, name)) in self.spec.parts.iter().zip(&self.names).enumerate() {
            if part.flags & u64::from(elf::SHF_ALLOC) == 0 {
                continue;
            }
            let ram = part.address >= RAM;
            let (chip, base) = if ram { ("ram", RAM) } else { ("flash", ROM) };
            writeln!(
                result,
                "| located | {chip} | {name} ({at}) | 0x{:x} | 0x{:x} | 0x{:x} | 0x4 |",
                part.address,
                part.address - base,
                part.size
            )
            .unwrap();
            if !name.starts_with("$fill") {
                used[usize::from(ram)] += part.size;
            }
        }
        result.push_str("* Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n");
        writeln!(
            result,
            "| flash | {} | 0 | 0 | {} | {ROM_SIZE} |",
            used[0],
            ROM_SIZE - used[0]
        )
        .unwrap();
        writeln!(
            result,
            "| ram | 0 | {} | 0 | {} | {RAM_SIZE} |",
            used[1],
            RAM_SIZE - used[1]
        )
        .unwrap();
        result
    }

    fn symbols(&mut self) -> &mut Vec<Named> {
        self.spec.symbols.as_mut().unwrap()
    }

    fn pad(&mut self, bytes: usize) {
        self.push(
            ".synthetic.payload".into(),
            Part {
                flags: 0,
                ..data(0, bytes)
            },
        );
    }
}

fn local(mut symbol: Named) -> Named {
    symbol.bind = elf::STB_LOCAL;
    symbol
}

fn file(module: &str) -> Named {
    Named {
        kind: elf::STT_FILE,
        part: None,
        ..local(func(&format!("{module}_Unit.o"), 0, 0, 0))
    }
}

fn split_functions(fixture: &mut Fixture) -> Vec<(u64, u64, u64)> {
    let mut rows = Vec::new();
    for at in 0..3000 {
        let module = MODULES[at % 40];
        let address = ROM + at as u64 * 64;
        let section = fixture.push(format!(".text.{module}.Func{at}"), text(address, 64));
        if at < 2500 {
            fixture.symbols().push(file(module));
            fixture.symbols().push(local(func(
                &format!("{module}_Func{at}"),
                address,
                32,
                section,
            )));
        }
        rows.extend(
            (0..64)
                .step_by(4)
                .map(|offset| (address + offset, 4, at as u64 + 1)),
        );
    }
    rows
}

fn split_data(fixture: &mut Fixture) {
    for at in 0..100 {
        let module = MODULES[at % 40];
        let ram = RAM + at as u64 * 16;
        let data_at = fixture.push(format!(".data.{module}.State{at}"), data(ram, 16));
        fixture.push(
            format!("[.data.{module}.State{at}]"),
            Part {
                flags: 2,
                ..data(ROM + 0x4_0000 + at as u64 * 16, 16)
            },
        );
        let zero_at = fixture.push(
            format!(".bss.{module}.Buffer{at}"),
            bss(RAM + 0x1_0000 + at as u64 * 64, 64),
        );
        fixture.symbols().push(local(object(
            &format!("{module}_State{at}"),
            ram,
            16,
            data_at,
        )));
        fixture.symbols().push(local(object(
            &format!("{module}_Buffer{at}"),
            RAM + 0x1_0000 + at as u64 * 64,
            64,
            zero_at,
        )));
    }
}

pub fn split_layout(symbols: usize) -> Fixture {
    assert!(symbols >= 6200);
    let mut fixture = Fixture::new();
    let rows = split_functions(&mut fixture);
    split_data(&mut fixture);
    for at in 0..9 {
        fixture.push(
            format!("$fill{at}"),
            Part {
                flags: 2,
                ..data(ROM + 0x10_0000 + at * 0x1_0000, 0x1_0000)
            },
        );
    }
    for at in 0..3500 {
        fixture
            .symbols()
            .push(local(absolute(&format!("Synthetic_Constant{at}"), at)));
    }
    for at in 0..symbols - 6200 {
        let section = at % 3000;
        fixture.symbols().push(local(func(
            &format!(".L{at:06}"),
            ROM + section as u64 * 64 + (at % 32) as u64,
            0,
            section,
        )));
    }
    fixture.spec.debug = line_program(false, false, &rows);
    fixture.pad(26_000_000);
    fixture
}

fn merged_sections(fixture: &mut Fixture) -> usize {
    fixture.push(".text".into(), text(ROM, 0x10_0000));
    fixture.push(
        ".rodata".into(),
        Part {
            flags: 2,
            ..data(ROM + 0x10_0000, 0x2_0000)
        },
    );
    let init = fixture.push(
        ".data".into(),
        Part {
            load_address: ROM + 0x20_0000,
            ..data(RAM, 0x1_0000)
        },
    );
    for at in 0..100 {
        let section = fixture.push(
            format!(".bss.Stack{at}"),
            bss(RAM + 0x1_0000 + at * 256, 256),
        );
        fixture.symbols().push(local(object(
            &format!("{}_Stack{at}", MODULES[at as usize % 40]),
            RAM + 0x1_0000 + at * 256,
            256,
            section,
        )));
    }
    fixture.spec.loads.truncate(50);
    init
}

fn merged_functions(fixture: &mut Fixture) -> Vec<(u64, u64, u64)> {
    let mut rows = Vec::new();
    for at in 0..4400 {
        let module = MODULES[at % 40];
        let address = ROM + at as u64 * 192;
        fixture.symbols().push(file(module));
        fixture
            .symbols()
            .push(local(func(&format!("{module}_Func{at}"), address, 128, 0)));
        rows.extend(
            (0..192)
                .step_by(8)
                .map(|offset| (address + offset, 8, at as u64 + 1)),
        );
    }
    rows
}

pub fn merged_layout(symbols: usize) -> Fixture {
    merged_with(symbols, 0, 24_000_000)
}

pub fn estimated_layout(symbols: usize, estimated: usize) -> Fixture {
    merged_with(symbols, estimated, 20_000_000)
}

fn merged_with(symbols: usize, estimated: usize, pad: usize) -> Fixture {
    assert!(symbols >= 11_000 + estimated);
    let mut fixture = Fixture::new();
    let init = merged_sections(&mut fixture);
    let rows = merged_functions(&mut fixture);
    for at in 0..500 {
        fixture.symbols().push(local(object(
            &format!("{}_State{at}", MODULES[at % 40]),
            RAM + at as u64 * 64,
            64,
            init,
        )));
    }
    for at in 0..6000 {
        fixture
            .symbols()
            .push(local(absolute(&format!("Synthetic_Constant{at}"), at)));
    }
    for at in 0..symbols - 11_000 - estimated {
        fixture.symbols().push(local(func(
            &format!(".L{at:06}"),
            ROM + (at % 4400) as u64 * 192 + (at % 128) as u64,
            0,
            0,
        )));
    }
    for at in 0..estimated {
        fixture.symbols().push(func(
            &format!("Synthetic_Entry{at}"),
            ROM + (at % 4400) as u64 * 192 + 130,
            0,
            0,
        ));
    }
    fixture.spec.debug = line_program(false, false, &rows);
    fixture.pad(pad);
    fixture
}

pub fn small(is_64: bool, is_big: bool, stripped: bool, dwarf: bool) -> Fixture {
    let mut fixture = Fixture::new();
    fixture.spec.is_64 = is_64;
    fixture.spec.is_big = is_big;
    fixture.push(".text.Com.Init".into(), text(ROM, 64));
    fixture.push(".text.Lamp.Read".into(), text(ROM + 64, 64));
    fixture.push(".data.Com.State".into(), data(RAM, 16));
    fixture.push(
        "[.data.Com.State]".into(),
        Part {
            flags: 2,
            ..data(ROM + 128, 16)
        },
    );
    fixture.push(".bss.Lamp.Buffer".into(), bss(RAM + 16, 64));
    fixture.push(
        "$fill".into(),
        Part {
            flags: 2,
            ..data(ROM + 144, 16)
        },
    );
    fixture.spec.symbols = (!stripped).then(|| {
        vec![
            func("Com_Init", ROM, 32, 0),
            func("Lamp_Read", ROM + 64, 32, 1),
        ]
    });
    if dwarf {
        fixture.spec.debug = line_program(is_64, is_big, &[(ROM, 64, 1), (ROM + 64, 64, 2)]);
    }
    fixture
}

mod writer {
    use object::{
        Endianness, elf,
        write::{
            StringId,
            elf::{FileHeader, ProgramHeader, SectionHeader, SectionIndex, Sym, Writer},
        },
    };

    use super::super::elf_build::Spec;

    struct Reserved {
        sections: Vec<(StringId, SectionIndex, usize)>,
        symbols: Vec<StringId>,
    }

    fn reserve<'a>(writer: &mut Writer<'a>, spec: &'a Spec, names: &'a [String]) -> Reserved {
        writer.reserve_file_header();
        writer.reserve_program_headers(spec.loads.len() as u32);
        writer.reserve_null_section_index();
        let mut sections: Vec<_> = names
            .iter()
            .map(|name| {
                (
                    writer.add_section_name(name.as_bytes()),
                    writer.reserve_section_index(),
                    0,
                )
            })
            .collect();
        sections.extend(spec.debug.iter().map(|(name, _)| {
            (
                writer.add_section_name(name.as_bytes()),
                writer.reserve_section_index(),
                0,
            )
        }));
        let mut symbols = Vec::new();
        if let Some(named) = &spec.symbols {
            writer.reserve_null_symbol_index();
            for symbol in named {
                symbols.push(writer.add_string(symbol.name.as_bytes()));
                writer.reserve_symbol_index(symbol.part.map(|at| sections[at].1));
            }
            writer.reserve_symtab_section_index();
            writer.reserve_strtab_section_index();
        }
        writer.reserve_shstrtab_section_index();
        for (section, part) in sections.iter_mut().zip(&spec.parts) {
            section.2 = writer.reserve(part.data.len(), 4);
        }
        for (section, (_, bytes)) in sections[spec.parts.len()..].iter_mut().zip(&spec.debug) {
            section.2 = writer.reserve(bytes.len(), 1);
        }
        if spec.symbols.is_some() {
            writer.reserve_symtab();
            writer.reserve_strtab();
        }
        writer.reserve_shstrtab();
        writer.reserve_section_headers();
        Reserved { sections, symbols }
    }

    fn contents(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
        writer
            .write_file_header(&FileHeader {
                os_abi: 0,
                abi_version: 0,
                e_type: elf::ET_EXEC,
                e_machine: spec.machine,
                e_entry: spec.entry,
                e_flags: 0,
            })
            .unwrap();
        writer.write_align_program_headers();
        for (at, flags) in &spec.loads {
            let part = &spec.parts[*at];
            writer.write_program_header(&ProgramHeader {
                p_type: elf::PT_LOAD,
                p_flags: *flags,
                p_offset: reserved.sections[*at].2 as u64,
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
        symbols(writer, spec, reserved);
        writer.write_shstrtab();
    }

    fn symbols(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
        let Some(symbols) = &spec.symbols else {
            return;
        };
        writer.write_null_symbol();
        for (symbol, name) in symbols.iter().zip(&reserved.symbols) {
            writer.write_symbol(&Sym {
                name: Some(*name),
                section: symbol.part.map(|at| reserved.sections[at].1),
                st_info: (symbol.bind << 4) | symbol.kind,
                st_other: 0,
                st_shndx: if symbol.is_absolute || symbol.kind == elf::STT_FILE {
                    elf::SHN_ABS
                } else {
                    0
                },
                st_value: symbol.value,
                st_size: symbol.size,
            });
        }
        writer.write_strtab();
    }

    fn headers(writer: &mut Writer<'_>, spec: &Spec, reserved: &Reserved) {
        writer.write_null_section_header();
        for (part, (name, _, offset)) in spec.parts.iter().zip(&reserved.sections) {
            writer.write_section_header(&SectionHeader {
                name: Some(*name),
                sh_type: part.kind,
                sh_flags: part.flags,
                sh_addr: part.address,
                sh_offset: *offset as u64,
                sh_size: part.size,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 4,
                sh_entsize: 0,
            });
        }
        for ((_, bytes), (name, _, offset)) in spec
            .debug
            .iter()
            .zip(&reserved.sections[spec.parts.len()..])
        {
            writer.write_section_header(&SectionHeader {
                name: Some(*name),
                sh_type: elf::SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_offset: *offset as u64,
                sh_size: bytes.len() as u64,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
            });
        }
        if let Some(symbols) = &spec.symbols {
            let locals = symbols
                .iter()
                .take_while(|symbol| symbol.bind == elf::STB_LOCAL)
                .count();
            writer.write_symtab_section_header(locals as u32 + 1);
            writer.write_strtab_section_header();
        }
        writer.write_shstrtab_section_header();
    }

    pub(super) fn build(spec: &Spec, names: &[String]) -> Vec<u8> {
        assert_eq!(spec.parts.len(), names.len());
        let endian = if spec.is_big {
            Endianness::Big
        } else {
            Endianness::Little
        };
        let mut buffer = Vec::new();
        let mut writer = Writer::new(endian, spec.is_64, &mut buffer);
        let reserved = reserve(&mut writer, spec, names);
        contents(&mut writer, spec, &reserved);
        headers(&mut writer, spec, &reserved);
        buffer
    }
}
