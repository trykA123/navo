pub const SUMMARY_HEAD: usize = 4096;

const ELF_HEADER: usize = 52;
const FAT_LIMIT: u32 = 40;

#[must_use]
pub fn binary_summary(head: &[u8]) -> Option<String> {
    if head.starts_with(b"MZ") {
        pe(head)
    } else if head.starts_with(b"\x7fELF") {
        elf(head)
    } else if head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        universal(head)
    } else {
        macho(head)
    }
}

fn le16(head: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?))
}

fn be16(head: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(head.get(at..at + 2)?.try_into().ok()?))
}

fn le32(head: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?))
}

fn be32(head: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(head.get(at..at + 4)?.try_into().ok()?))
}

fn pe(head: &[u8]) -> Option<String> {
    let at = usize::try_from(le32(head, 0x3C)?).ok()?;
    if head.get(at..at + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = le16(head, at + 4)?;
    let flags = le16(head, at + 22)?;
    let format = match le16(head, at + 24)? {
        0x10b => "PE32",
        0x20b => "PE32+",
        _ => return None,
    };
    let machine = match machine {
        0x14c => "x86".to_owned(),
        0x8664 => "x86-64".to_owned(),
        0xAA64 => "ARM64".to_owned(),
        0x1c0 | 0x1c4 => "ARM".to_owned(),
        other => format!("machine {other:#x}"),
    };
    let kind = if flags & 0x2000 != 0 {
        "DLL"
    } else if flags & 0x0002 != 0 {
        "program"
    } else {
        "object"
    };
    Some(format!("{format} · {machine} · {kind}"))
}

fn elf(head: &[u8]) -> Option<String> {
    if head.len() < ELF_HEADER {
        return None;
    }
    let bits = match head[4] {
        1 => "32-bit",
        2 => "64-bit",
        _ => return None,
    };
    let read: fn(&[u8], usize) -> Option<u16> = match head[5] {
        1 => le16,
        2 => be16,
        _ => return None,
    };
    let kind = match read(head, 16)? {
        1 => "object",
        2 => "program",
        3 => "shared object",
        4 => "core dump",
        _ => "other",
    };
    let machine = match read(head, 18)? {
        62 => "x86-64".to_owned(),
        3 => "x86".to_owned(),
        183 => "ARM64".to_owned(),
        40 => "ARM".to_owned(),
        243 => "RISC-V".to_owned(),
        other => format!("machine {other}"),
    };
    Some(format!("ELF {bits} · {machine} · {kind}"))
}

fn macho(head: &[u8]) -> Option<String> {
    let bits = match le32(head, 0)? {
        0xFEED_FACE => "32-bit",
        0xFEED_FACF => "64-bit",
        _ => return None,
    };
    let machine = match le32(head, 4)? {
        0x0100_0007 => "x86-64".to_owned(),
        7 => "x86".to_owned(),
        0x0100_000C => "ARM64".to_owned(),
        12 => "ARM".to_owned(),
        other => format!("machine {other:#x}"),
    };
    let kind = match le32(head, 12)? {
        1 => "object",
        2 => "program",
        6 => "shared library",
        8 => "bundle",
        _ => "other",
    };
    Some(format!("Mach-O {bits} · {machine} · {kind}"))
}

fn universal(head: &[u8]) -> Option<String> {
    match be32(head, 4)? {
        1 => Some("Mach-O universal · 1 architecture".to_owned()),
        count if count > 1 && count < FAT_LIMIT => {
            Some(format!("Mach-O universal · {count} architectures"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
