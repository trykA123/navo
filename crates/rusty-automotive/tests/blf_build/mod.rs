#![allow(
    dead_code,
    clippy::unwrap_used,
    reason = "invented BLF fixture builders shared by test binaries"
)]

use std::io::Write;
use std::sync::atomic::AtomicBool;

use flate2::{Compression, write::ZlibEncoder};
use rusty_automotive::{Body, Entry, Frame, Trace, Watch, index_blf};

pub fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn object(kind: u32, version: u16, timestamp: u64, payload: &[u8]) -> Vec<u8> {
    let header_size = if version == 2 { 40 } else { 32 };
    let mut bytes = vec![0; header_size];
    bytes[..4].copy_from_slice(b"LOBJ");
    put16(&mut bytes, 4, header_size as u16);
    put16(&mut bytes, 6, version);
    put32(&mut bytes, 8, (header_size + payload.len()) as u32);
    put32(&mut bytes, 12, kind);
    put32(&mut bytes, 16, if version == 2 { 1 } else { 2 });
    bytes[24..32].copy_from_slice(&timestamp.to_le_bytes());
    bytes.extend(payload);
    bytes
}

pub fn classic(kind: u32, flags: u8, raw_id: u32) -> Vec<u8> {
    let mut bytes = vec![0; if kind == 86 { 24 } else { 16 }];
    put16(&mut bytes, 0, 2);
    bytes[2] = flags;
    bytes[3] = 8;
    put32(&mut bytes, 4, raw_id);
    for (at, byte) in bytes[8..16].iter_mut().enumerate() {
        *byte = at as u8;
    }
    bytes
}

pub fn fd(kind: u32, flags: u32) -> Vec<u8> {
    if kind == 100 {
        let mut bytes = vec![0; 88];
        put16(&mut bytes, 0, 3);
        bytes[2] = 1;
        bytes[3] = 15;
        put32(&mut bytes, 4, (1 << 31) | 32);
        bytes[13] = flags as u8;
        bytes[14] = 64;
        for (at, byte) in bytes[20..84].iter_mut().enumerate() {
            *byte = at as u8;
        }
        return bytes;
    }
    let mut bytes = vec![0; 104];
    bytes[0] = 4;
    bytes[1] = 15;
    bytes[2] = 64;
    put32(&mut bytes, 4, 48);
    put32(&mut bytes, 12, flags);
    bytes[34] = 1;
    for (at, byte) in bytes[40..104].iter_mut().enumerate() {
        *byte = at as u8;
    }
    bytes
}

pub fn error() -> Vec<u8> {
    let mut bytes = vec![0; 32];
    put16(&mut bytes, 0, 5);
    put32(&mut bytes, 4, 2);
    bytes[10] = 3;
    put32(&mut bytes, 16, (1 << 31) | 64);
    put16(&mut bytes, 20, 3 << 12);
    bytes[24..27].copy_from_slice(&[4, 5, 6]);
    bytes
}

pub fn container(inner: &[u8], compressed: bool) -> Vec<u8> {
    let data = if compressed {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(inner).unwrap();
        encoder.finish().unwrap()
    } else {
        inner.to_vec()
    };
    let mut bytes = vec![0; 32];
    bytes[..4].copy_from_slice(b"LOBJ");
    put16(&mut bytes, 4, 16);
    put16(&mut bytes, 6, 1);
    put32(&mut bytes, 8, (32 + data.len()) as u32);
    put32(&mut bytes, 12, 10);
    put16(&mut bytes, 16, if compressed { 2 } else { 0 });
    put32(&mut bytes, 24, inner.len() as u32);
    bytes.extend(data);
    bytes.extend(std::iter::repeat_n(0, bytes.len() % 4));
    bytes
}

pub fn file(containers: &[Vec<u8>], count: u32) -> Vec<u8> {
    let mut bytes = vec![0; 144];
    bytes[..4].copy_from_slice(b"LOGG");
    put32(&mut bytes, 4, 144);
    put32(&mut bytes, 32, count);
    for offset in [40, 56] {
        for (at, field) in [2026, 10, 2, 20, 12, 34, 56, 789].into_iter().enumerate() {
            put16(&mut bytes, offset + at * 2, field);
        }
    }
    for container in containers {
        bytes.extend(container);
    }
    let size = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&size.to_le_bytes());
    bytes
}

pub fn trace(bytes: &[u8]) -> Trace {
    index_blf(
        std::io::Cursor::new(bytes),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap()
}

pub fn entries(bytes: &[u8], trace: &Trace, rows: &[u64]) -> Vec<Entry> {
    trace
        .plan(rows)
        .decode(&mut |at, len| {
            let start = (at as usize).min(bytes.len());
            Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
        })
        .unwrap()
}

pub fn frame(entry: &Entry) -> &Frame {
    let Body::Frame(frame) = &entry.body else {
        unreachable!()
    };
    frame
}
