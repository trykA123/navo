use std::fmt::Write;

pub const HEX_WIDTH: u64 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexRow {
    pub offset: String,
    pub hex: String,
    pub ascii: String,
}

#[must_use]
pub fn hex_rows(len: u64) -> u64 {
    len.div_ceil(HEX_WIDTH)
}

#[must_use]
pub fn hex_row_start(offset: u64) -> u64 {
    offset - offset % HEX_WIDTH
}

#[must_use]
pub fn hex_row(offset: u64, bytes: &[u8], wide: bool) -> HexRow {
    let offset = if wide {
        format!("{offset:016X}")
    } else {
        format!("{offset:08X}")
    };
    let mut hex = String::with_capacity(48);
    for slot in 0..16 {
        if slot > 0 {
            hex.push(' ');
        }
        if slot == 8 {
            hex.push(' ');
        }
        match bytes.get(slot) {
            Some(byte) => {
                let _ = write!(hex, "{byte:02X}");
            }
            None => hex.push_str("  "),
        }
    }
    let ascii = bytes
        .iter()
        .take(16)
        .map(|&byte| {
            if (0x20..=0x7E).contains(&byte) {
                char::from(byte)
            } else {
                '.'
            }
        })
        .collect();
    HexRow { offset, hex, ascii }
}
