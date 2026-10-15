use super::binary_summary;

const PE_OFFSET: usize = 0x80;

fn put16(buf: &mut [u8], at: usize, value: u16) {
    buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn pe(plus: bool, machine: u16, characteristics: u16) -> Vec<u8> {
    let optional_size: u16 = if plus { 240 } else { 224 };
    let mut buf = vec![0u8; 1024];
    buf[0..2].copy_from_slice(b"MZ");
    put32(&mut buf, 0x3c, PE_OFFSET as u32);
    buf[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    let coff = PE_OFFSET + 4;
    put16(&mut buf, coff, machine);
    put16(&mut buf, coff + 16, optional_size);
    put16(&mut buf, coff + 18, characteristics);
    let optional = coff + 20;
    put16(&mut buf, optional, if plus { 0x20b } else { 0x10b });
    let rva_count = optional + if plus { 108 } else { 92 };
    put32(&mut buf, rva_count, 16);
    buf
}

fn elf(class: u8, data: u8, e_type: u16, machine: u16) -> Vec<u8> {
    let wide = class == 2;
    let mut buf = vec![0u8; if wide { 64 } else { 52 }];
    buf[0..4].copy_from_slice(b"\x7fELF");
    buf[4] = class;
    buf[5] = data;
    buf[6] = 1;
    let half = |value: u16| {
        if data == 2 {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        }
    };
    buf[16..18].copy_from_slice(&half(e_type));
    buf[18..20].copy_from_slice(&half(machine));
    let word = if data == 2 {
        1u32.to_be_bytes()
    } else {
        1u32.to_le_bytes()
    };
    buf[20..24].copy_from_slice(&word);
    let ehsize_at = if wide { 52 } else { 40 };
    let size = u16::try_from(buf.len()).unwrap();
    buf[ehsize_at..ehsize_at + 2].copy_from_slice(&half(size));
    buf
}

fn macho(wide: bool, cputype: u32, filetype: u32) -> Vec<u8> {
    let mut buf = vec![0u8; 64];
    put32(&mut buf, 0, if wide { 0xFEED_FACF } else { 0xFEED_FACE });
    put32(&mut buf, 4, cputype);
    put32(&mut buf, 12, filetype);
    buf
}

fn universal(count: u32) -> Vec<u8> {
    let mut buf = vec![0xCA, 0xFE, 0xBA, 0xBE];
    buf.extend_from_slice(&count.to_be_bytes());
    let cputypes = [0x0100_0007u32, 0x0100_000C];
    for index in 0..count.min(39) {
        let cputype = cputypes[index as usize % cputypes.len()];
        let offset = 0x4000 * (index + 1);
        for field in [cputype, 3, offset, 0x1000, 14] {
            buf.extend_from_slice(&field.to_be_bytes());
        }
    }
    buf
}

fn summary(head: &[u8]) -> Option<String> {
    binary_summary(head)
}

fn some(text: &str) -> Option<String> {
    Some(text.to_owned())
}

#[test]
fn pe32_x86_program() {
    assert_eq!(
        summary(&pe(false, 0x14c, 0x0102)),
        some("PE32 · x86 · program")
    );
}

#[test]
fn pe32_plus_x86_64_dll() {
    assert_eq!(
        summary(&pe(true, 0x8664, 0x2022)),
        some("PE32+ · x86-64 · DLL")
    );
}

#[test]
fn pe_dll_flag_wins_over_executable_flag() {
    assert_eq!(
        summary(&pe(true, 0xAA64, 0x2002)),
        some("PE32+ · ARM64 · DLL")
    );
}

#[test]
fn pe_without_executable_or_dll_flag_is_an_object() {
    assert_eq!(
        summary(&pe(true, 0x8664, 0x0020)),
        some("PE32+ · x86-64 · object")
    );
}

#[test]
fn pe_arm_machines() {
    assert_eq!(
        summary(&pe(false, 0x1c0, 0x0002)),
        some("PE32 · ARM · program")
    );
    assert_eq!(
        summary(&pe(false, 0x1c4, 0x0002)),
        some("PE32 · ARM · program")
    );
    assert_eq!(
        summary(&pe(true, 0xAA64, 0x0022)),
        some("PE32+ · ARM64 · program")
    );
}

#[test]
fn pe_unknown_machine_is_lowercase_hex() {
    assert_eq!(
        summary(&pe(true, 0x5A6B, 0x0002)),
        some("PE32+ · machine 0x5a6b · program")
    );
    assert_eq!(
        summary(&pe(false, 0x0EBC, 0x2000)),
        some("PE32 · machine 0xebc · DLL")
    );
}

#[test]
fn elf_64_bit_x86_64_shared_object() {
    assert_eq!(
        summary(&elf(2, 1, 3, 62)),
        some("ELF 64-bit · x86-64 · shared object")
    );
}

#[test]
fn elf_types() {
    assert_eq!(
        summary(&elf(2, 1, 1, 183)),
        some("ELF 64-bit · ARM64 · object")
    );
    assert_eq!(
        summary(&elf(2, 1, 2, 243)),
        some("ELF 64-bit · RISC-V · program")
    );
    assert_eq!(
        summary(&elf(2, 1, 4, 62)),
        some("ELF 64-bit · x86-64 · core dump")
    );
    assert_eq!(
        summary(&elf(2, 1, 0xfe00, 62)),
        some("ELF 64-bit · x86-64 · other")
    );
}

#[test]
fn elf_32_bit_machines() {
    assert_eq!(
        summary(&elf(1, 1, 2, 3)),
        some("ELF 32-bit · x86 · program")
    );
    assert_eq!(
        summary(&elf(1, 1, 3, 40)),
        some("ELF 32-bit · ARM · shared object")
    );
}

#[test]
fn elf_unknown_machine_is_decimal() {
    assert_eq!(
        summary(&elf(1, 1, 2, 8)),
        some("ELF 32-bit · machine 8 · program")
    );
}

#[test]
fn elf_big_endian_fields_are_read_big_endian() {
    assert_eq!(
        summary(&elf(1, 2, 2, 40)),
        some("ELF 32-bit · ARM · program")
    );
    assert_eq!(
        summary(&elf(2, 2, 3, 21)),
        some("ELF 64-bit · machine 21 · shared object")
    );
}

#[test]
fn macho_64_bit_arm64_program() {
    assert_eq!(
        summary(&macho(true, 0x0100_000C, 2)),
        some("Mach-O 64-bit · ARM64 · program")
    );
}

#[test]
fn macho_file_types_and_machines() {
    assert_eq!(
        summary(&macho(true, 0x0100_0007, 6)),
        some("Mach-O 64-bit · x86-64 · shared library")
    );
    assert_eq!(
        summary(&macho(false, 7, 1)),
        some("Mach-O 32-bit · x86 · object")
    );
    assert_eq!(
        summary(&macho(false, 12, 8)),
        some("Mach-O 32-bit · ARM · bundle")
    );
    assert_eq!(
        summary(&macho(true, 0x0100_000C, 5)),
        some("Mach-O 64-bit · ARM64 · other")
    );
}

#[test]
fn macho_unknown_cputype_is_lowercase_hex() {
    assert_eq!(
        summary(&macho(false, 0x12, 2)),
        some("Mach-O 32-bit · machine 0x12 · program")
    );
}

#[test]
fn macho_universal_counts_architectures() {
    assert_eq!(
        summary(&universal(2)),
        some("Mach-O universal · 2 architectures")
    );
    assert_eq!(
        summary(&universal(39)),
        some("Mach-O universal · 39 architectures")
    );
}

#[test]
fn a_java_class_is_not_a_universal_binary() {
    let class = [0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52, 0x0A, 0, 0x1D, 0];
    assert_eq!(summary(&class), None);
    assert_eq!(summary(&universal(40)), None);
}

#[test]
fn non_executables_have_no_summary() {
    assert_eq!(summary(b""), None);
    assert_eq!(summary(b"fn main() {}\nplain text, not a binary\n"), None);
    assert_eq!(
        summary(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10\x08\x06\0\0\0"),
        None
    );
    assert_eq!(summary(b"MZ"), None);
}

#[test]
fn truncated_pe_has_no_summary() {
    let mut past_end = pe(true, 0x8664, 0x0022);
    put32(&mut past_end, 0x3c, 0x2000);
    assert_eq!(summary(&past_end), None);
    let mut no_signature = pe(true, 0x8664, 0x0022);
    no_signature[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"XX\0\0");
    assert_eq!(summary(&no_signature), None);
    let cut = pe(true, 0x8664, 0x0022);
    assert_eq!(summary(&cut[..PE_OFFSET + 2]), None);
}

#[test]
fn truncated_elf_has_no_summary() {
    let full = elf(2, 1, 3, 62);
    assert_eq!(summary(&full[..51]), None);
    assert_eq!(summary(&full[..20]), None);
    assert_eq!(summary(b"\x7fELF"), None);
}

#[cfg(target_os = "linux")]
#[test]
fn bin_sh_is_a_64_bit_elf() {
    let Ok(bytes) = std::fs::read("/bin/sh") else {
        return;
    };
    let head = &bytes[..bytes.len().min(super::SUMMARY_HEAD)];
    let text = summary(head).expect("/bin/sh has a summary");
    assert!(text.starts_with("ELF 64-bit · "), "{text}");
}

#[cfg(windows)]
fn system32(name: &str) -> Option<Vec<u8>> {
    let mut bytes = std::fs::read(format!(r"C:\Windows\System32\{name}")).ok()?;
    bytes.truncate(super::SUMMARY_HEAD);
    Some(bytes)
}

#[cfg(windows)]
#[test]
fn kernel32_is_a_64_bit_dll() {
    let Some(head) = system32("kernel32.dll") else {
        return;
    };
    let text = summary(&head).expect("kernel32.dll has a summary");
    assert!(
        text == "PE32+ · x86-64 · DLL" || text == "PE32+ · ARM64 · DLL",
        "{text}"
    );
}

#[cfg(windows)]
#[test]
fn notepad_is_a_64_bit_program() {
    let Some(head) = system32("notepad.exe") else {
        return;
    };
    let text = summary(&head).expect("notepad.exe has a summary");
    assert!(
        text.starts_with("PE32+ · ") && text.ends_with(" · program"),
        "{text}"
    );
}
