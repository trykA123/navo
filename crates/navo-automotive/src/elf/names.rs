//! Names ELF values for the automotive viewer.

use object::elf;

const MACHINES: [(u16, &str); 34] = [
    (elf::EM_SPARC.0, "SPARC"),
    (elf::EM_386.0, "x86"),
    (elf::EM_68K.0, "Motorola 68000"),
    (elf::EM_MIPS.0, "MIPS"),
    (elf::EM_PPC.0, "PowerPC"),
    (elf::EM_PPC64.0, "PowerPC 64"),
    (elf::EM_S390.0, "IBM S/390"),
    (elf::EM_V800.0, "Renesas V800 / RH850"),
    (elf::EM_ARM.0, "ARM"),
    (elf::EM_SH.0, "Renesas SuperH"),
    (elf::EM_TRICORE.0, "Infineon TriCore"),
    (elf::EM_ARC.0, "ARC"),
    (elf::EM_H8_300.0, "Renesas H8/300"),
    (elf::EM_H8S.0, "Renesas H8S"),
    (elf::EM_68HC12.0, "Motorola 68HC12"),
    (elf::EM_X86_64.0, "x86-64"),
    (elf::EM_AVR.0, "AVR"),
    (elf::EM_V850.0, "Renesas V850 / RH850"),
    (elf::EM_XTENSA.0, "Xtensa"),
    (elf::EM_MSP430.0, "MSP430"),
    (elf::EM_C166.0, "Infineon C16x"),
    (elf::EM_M32C.0, "Renesas M32C"),
    (elf::EM_TI_C6000.0, "TI C6000"),
    (elf::EM_TI_C2000.0, "TI C2000"),
    (elf::EM_RX.0, "Renesas RX"),
    (elf::EM_AARCH64.0, "AArch64"),
    (elf::EM_RL78.0, "Renesas RL78"),
    (elf::EM_ARC_COMPACT2.0, "ARCv2"),
    (elf::EM_RISCV.0, "RISC-V"),
    (elf::EM_LOONGARCH.0, "LoongArch"),
    (elf::EM_SPARCV9.0, "SPARC v9"),
    (elf::EM_ARC_COMPACT.0, "ARCompact"),
    (elf::EM_56800EX.0, "NXP 56800EX"),
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

/// Names a numeric ELF file type, returning a fallback for unknown types.
pub(super) fn file_kind(kind: u16) -> &'static str {
    match elf::FileType(kind) {
        elf::ET_REL => "relocatable",
        elf::ET_EXEC => "executable",
        elf::ET_DYN => "shared object",
        elf::ET_CORE => "core dump",
        _ => "unknown type",
    }
}

const SECTION_KINDS: [(u32, &str); 12] = [
    (elf::SHT_NULL.0, "NULL"),
    (elf::SHT_PROGBITS.0, "PROGBITS"),
    (elf::SHT_SYMTAB.0, "SYMTAB"),
    (elf::SHT_STRTAB.0, "STRTAB"),
    (elf::SHT_RELA.0, "RELA"),
    (elf::SHT_HASH.0, "HASH"),
    (elf::SHT_DYNAMIC.0, "DYNAMIC"),
    (elf::SHT_NOTE.0, "NOTE"),
    (elf::SHT_NOBITS.0, "NOBITS"),
    (elf::SHT_REL.0, "REL"),
    (elf::SHT_INIT_ARRAY.0, "INIT_ARRAY"),
    (elf::SHT_FINI_ARRAY.0, "FINI_ARRAY"),
];

pub(super) fn section_kind(kind: u32) -> String {
    SECTION_KINDS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or_else(|| format!("0x{kind:X}"), |(_, name)| (*name).to_owned())
}

/// Formats numeric ELF section flags as the existing viewer letters.
pub(super) fn section_flags(flags: u64) -> String {
    [
        (elf::SHF_WRITE.0, 'W'),
        (elf::SHF_ALLOC.0, 'A'),
        (elf::SHF_EXECINSTR.0, 'X'),
        (elf::SHF_MERGE.0, 'M'),
        (elf::SHF_STRINGS.0, 'S'),
        (elf::SHF_TLS.0, 'T'),
        (elf::SHF_COMPRESSED.0, 'C'),
    ]
    .into_iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, letter)| letter)
    .collect()
}

/// Formats numeric ELF segment flags as read, write, and execute letters.
pub(super) fn segment_flags(flags: u32) -> String {
    [(elf::PF_R.0, 'R'), (elf::PF_W.0, 'W'), (elf::PF_X.0, 'X')]
        .into_iter()
        .map(|(bit, letter)| if flags & bit != 0 { letter } else { '-' })
        .collect()
}
