use object::elf;

const MACHINES: [(u16, &str); 34] = [
    (elf::EM_SPARC, "SPARC"),
    (elf::EM_386, "x86"),
    (elf::EM_68K, "Motorola 68000"),
    (elf::EM_MIPS, "MIPS"),
    (elf::EM_PPC, "PowerPC"),
    (elf::EM_PPC64, "PowerPC 64"),
    (elf::EM_S390, "IBM S/390"),
    (elf::EM_V800, "Renesas V800 / RH850"),
    (elf::EM_ARM, "ARM"),
    (elf::EM_SH, "Renesas SuperH"),
    (elf::EM_TRICORE, "Infineon TriCore"),
    (elf::EM_ARC, "ARC"),
    (elf::EM_H8_300, "Renesas H8/300"),
    (elf::EM_H8S, "Renesas H8S"),
    (elf::EM_68HC12, "Motorola 68HC12"),
    (elf::EM_X86_64, "x86-64"),
    (elf::EM_AVR, "AVR"),
    (elf::EM_V850, "Renesas V850 / RH850"),
    (elf::EM_XTENSA, "Xtensa"),
    (elf::EM_MSP430, "MSP430"),
    (elf::EM_C166, "Infineon C16x"),
    (elf::EM_M32C, "Renesas M32C"),
    (elf::EM_TI_C6000, "TI C6000"),
    (elf::EM_TI_C2000, "TI C2000"),
    (elf::EM_RX, "Renesas RX"),
    (elf::EM_AARCH64, "AArch64"),
    (elf::EM_RL78, "Renesas RL78"),
    (elf::EM_ARC_COMPACT2, "ARCv2"),
    (elf::EM_RISCV, "RISC-V"),
    (elf::EM_LOONGARCH, "LoongArch"),
    (elf::EM_SPARCV9, "SPARC v9"),
    (elf::EM_ARC_COMPACT, "ARCompact"),
    (elf::EM_56800EX, "NXP 56800EX"),
    (0x9080, "Renesas V850 (old)"),
];

pub(super) fn machine(code: u16) -> String {
    MACHINES
        .iter()
        .find(|(known, _)| *known == code)
        .map_or_else(
            || format!("machine 0x{code:04X}"),
            |(_, name)| (*name).to_owned(),
        )
}

pub(super) fn file_kind(kind: u16) -> &'static str {
    match kind {
        elf::ET_REL => "relocatable",
        elf::ET_EXEC => "executable",
        elf::ET_DYN => "shared object",
        elf::ET_CORE => "core dump",
        _ => "unknown type",
    }
}

const SECTION_KINDS: [(u32, &str); 12] = [
    (elf::SHT_NULL, "NULL"),
    (elf::SHT_PROGBITS, "PROGBITS"),
    (elf::SHT_SYMTAB, "SYMTAB"),
    (elf::SHT_STRTAB, "STRTAB"),
    (elf::SHT_RELA, "RELA"),
    (elf::SHT_HASH, "HASH"),
    (elf::SHT_DYNAMIC, "DYNAMIC"),
    (elf::SHT_NOTE, "NOTE"),
    (elf::SHT_NOBITS, "NOBITS"),
    (elf::SHT_REL, "REL"),
    (elf::SHT_INIT_ARRAY, "INIT_ARRAY"),
    (elf::SHT_FINI_ARRAY, "FINI_ARRAY"),
];

pub(super) fn section_kind(kind: u32) -> String {
    SECTION_KINDS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or_else(|| format!("0x{kind:X}"), |(_, name)| (*name).to_owned())
}

pub(super) fn section_flags(flags: u64) -> String {
    [
        (u64::from(elf::SHF_WRITE), 'W'),
        (u64::from(elf::SHF_ALLOC), 'A'),
        (u64::from(elf::SHF_EXECINSTR), 'X'),
        (u64::from(elf::SHF_MERGE), 'M'),
        (u64::from(elf::SHF_STRINGS), 'S'),
        (u64::from(elf::SHF_TLS), 'T'),
        (u64::from(elf::SHF_COMPRESSED), 'C'),
    ]
    .into_iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, letter)| letter)
    .collect()
}

pub(super) fn segment_flags(flags: u32) -> String {
    [(elf::PF_R, 'R'), (elf::PF_W, 'W'), (elf::PF_X, 'X')]
        .into_iter()
        .map(|(bit, letter)| if flags & bit != 0 { letter } else { '-' })
        .collect()
}
