#![allow(
    dead_code,
    clippy::unwrap_used,
    reason = "test helpers; each test binary uses a part of them"
)]

use std::sync::atomic::AtomicBool;

use rusty_automotive::{Format, Image, Watch, index};

pub const INTEL_EXAMPLE: &str = "\
:10010000214601360121470136007EFE09D2190140\r
:100110002146017E17C20001FF5F16002148011928\r
:10012000194E79234623965778239EDA3F01B2CAA7\r
:100130003F0156702B5E712B722B732146013421C7\r
:00000001FF\r
";

pub const SREC_EXAMPLE: &str = "\
S00F000068656C6C6F202020202000003C
S11F00007C0802A6900100049421FFF07C6C1B787C8C23783C6000003863000026
S11F001C4BFFFFE5398000007D83637880010014382100107C0803A64E800020E9
S111003848656C6C6F20776F726C642E0A0042
S5030003F9
S9030000FC
";

fn hex(out: &mut String, bytes: &[u8]) {
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
}

fn intel_record(out: &mut String, kind: u8, address: u16, data: &[u8]) {
    let mut bytes = vec![data.len() as u8];
    bytes.extend(address.to_be_bytes());
    bytes.push(kind);
    bytes.extend(data);
    let sum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    bytes.push(0u8.wrapping_sub(sum));
    out.push(':');
    hex(out, &bytes);
    out.push('\n');
}

pub fn intel_hex(blocks: &[(u32, Vec<u8>)], with_end: bool) -> String {
    let mut out = String::new();
    let mut upper = None;
    for (address, data) in blocks {
        let mut at = u64::from(*address);
        let mut rest = data.as_slice();
        while !rest.is_empty() {
            let room = 0x1_0000 - (at & 0xFFFF);
            let take = rest.len().min(16).min(room as usize);
            let high = (at >> 16) as u16;
            if upper != Some(high) {
                intel_record(&mut out, 4, 0, &high.to_be_bytes());
                upper = Some(high);
            }
            let (chunk, after) = rest.split_at(take);
            intel_record(&mut out, 0, (at & 0xFFFF) as u16, chunk);
            at += take as u64;
            rest = after;
        }
    }
    if with_end {
        intel_record(&mut out, 1, 0, &[]);
    }
    out
}

fn s_record(out: &mut String, kind: u8, address: &[u8], data: &[u8]) {
    let mut bytes = vec![(address.len() + data.len() + 1) as u8];
    bytes.extend(address);
    bytes.extend(data);
    let sum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    bytes.push(!sum);
    out.push('S');
    out.push(char::from(b'0' + kind));
    hex(out, &bytes);
    out.push('\n');
}

pub fn s_records(blocks: &[(u32, Vec<u8>)], start: u32) -> String {
    let mut out = String::new();
    s_record(&mut out, 0, &[0, 0], b"fixture");
    for (address, data) in blocks {
        for (at, chunk) in data.chunks(32).enumerate() {
            let address = address + (at * 32) as u32;
            s_record(&mut out, 3, &address.to_be_bytes(), chunk);
        }
    }
    s_record(&mut out, 7, &start.to_be_bytes(), &[]);
    out
}

pub fn parse(format: Format, text: &str) -> Image {
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    index(format, text.as_bytes(), watch).unwrap()
}

pub fn read_all(text: &str, image: &Image, window: std::ops::Range<u64>) -> Vec<Option<u8>> {
    let bytes = text.as_bytes();
    let mut read = |offset: u64, len: usize| {
        let start = offset as usize;
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    };
    image.read(window, &mut read).unwrap()
}
