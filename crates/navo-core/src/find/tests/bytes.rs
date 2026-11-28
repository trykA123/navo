use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::find::{Compiled, FileHits, FindQuery, Skip, scan};

fn search_hex(bytes: &[u8], query: &str) -> FileHits {
    let compiled = Compiled::new(&FindQuery::parse(query)).unwrap();
    scan(bytes, &compiled, &AtomicBool::new(false)).unwrap()
}

#[test]
fn hex_matches_at_offset_zero_and_formats_sixteen_byte_rows() {
    let mut bytes = vec![
        0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00,
    ];
    bytes.extend_from_slice(&[
        0x00, 0x01, 0x02, 0x03, 0x04, 0x4D, 0x5A, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ]);
    bytes.extend_from_slice(&[0xAA, 0xBB, 0x4D, 0x5A, 0xCC]);

    let found = search_hex(&bytes, "hex:4D5A");
    assert_eq!(found.skipped, None);
    assert_eq!(found.total, 3);
    assert_eq!(found.hits.len(), 3);

    assert_eq!(found.hits[0].line, 1);
    assert_eq!(found.hits[0].column, 1);
    assert_eq!(
        found.hits[0].text,
        "00000000  4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00"
    );

    assert_eq!(found.hits[1].line, 2);
    assert_eq!(found.hits[1].column, 6);
    assert_eq!(
        found.hits[1].text,
        "00000010  00 01 02 03 04 4D 5A 07 08 09 0A 0B 0C 0D 0E 0F"
    );

    assert_eq!(found.hits[2].line, 3);
    assert_eq!(found.hits[2].column, 3);
    assert_eq!(found.hits[2].text, "00000020  AA BB 4D 5A CC");
}

#[test]
fn hex_matches_across_chunk_boundary_without_duplicates() {
    const CHUNK: usize = 1024 * 1024;
    let needle = [0x4D, 0x5A, 0x90, 0xEF];

    let mut straddle = vec![0x00; CHUNK + 32];
    straddle[CHUNK - 2..CHUNK + 2].copy_from_slice(&needle);
    let found = search_hex(&straddle, "hex:4D5A90EF");
    assert_eq!(found.skipped, None);
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].line, (CHUNK as u64 - 2) / 16 + 1);
    assert_eq!(found.hits[0].column, 15);
    assert_eq!(
        found.hits[0].text,
        "000FFFF0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 4D 5A"
    );

    let mut adjacent = vec![0x11; CHUNK + 32];
    adjacent[CHUNK - 4..CHUNK].copy_from_slice(&needle);
    adjacent[CHUNK..CHUNK + 4].copy_from_slice(&needle);
    let found = search_hex(&adjacent, "hex:4D5A90EF");
    assert_eq!(found.total, 2);
    assert_eq!(found.hits[0].line, (CHUNK as u64 - 4) / 16 + 1);
    assert_eq!(found.hits[0].column, 13);
    assert_eq!(found.hits[1].line, CHUNK as u64 / 16 + 1);
    assert_eq!(found.hits[1].column, 1);

    let mut repeated = vec![0x11; CHUNK + 16];
    repeated[CHUNK - 2..CHUNK + 1].copy_from_slice(&[0xAA, 0xAA, 0xAA]);
    let found = search_hex(&repeated, "hex:AAAA");
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].line, (CHUNK as u64 - 2) / 16 + 1);
    assert_eq!(found.hits[0].column, 15);
}

#[test]
fn hex_keeps_full_sixteen_byte_row_across_short_reads() {
    struct ShortReader<'a> {
        data: &'a [u8],
        step: usize,
    }
    impl Read for ShortReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.data.len().min(buffer.len()).min(self.step);
            buffer[..count].copy_from_slice(&self.data[..count]);
            self.data = &self.data[count..];
            Ok(count)
        }
    }

    let data: Vec<u8> = (0u8..32).collect();
    let compiled = Compiled::new(&FindQuery::parse("hex:0102")).unwrap();
    let reader = ShortReader {
        data: &data,
        step: 7,
    };
    let found = scan(reader, &compiled, &AtomicBool::new(false)).unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].line, 1);
    assert_eq!(found.hits[0].column, 2);
    assert_eq!(
        found.hits[0].text,
        "00000000  00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F"
    );
}

#[test]
fn hex_keeps_hit_limit_and_counts_all_matches() {
    let bytes = [0x4D, 0x5A].repeat(1005);
    let found = search_hex(&bytes, "hex:4D5A");
    assert_eq!(found.skipped, None);
    assert_eq!(found.hits.len(), 1000);
    assert_eq!(found.total, 1005);
}

#[test]
fn hex_honours_cancellation_before_and_during_scan() {
    struct CancellingReader<'a> {
        cancel: &'a AtomicBool,
        reads: usize,
    }
    impl Read for CancellingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            buffer.fill(0x4D);
            self.cancel.store(true, Ordering::Relaxed);
            Ok(buffer.len())
        }
    }

    let cancel = AtomicBool::new(false);
    let mut reader = CancellingReader {
        cancel: &cancel,
        reads: 0,
    };
    let compiled = Compiled::new(&FindQuery::parse("hex:4D")).unwrap();
    let found = scan(&mut reader, &compiled, &cancel).unwrap();
    assert_eq!(reader.reads, 1);
    assert_eq!(found.skipped, Some(Skip::Cancelled));

    let found = scan(&mut reader, &compiled, &cancel).unwrap();
    assert_eq!(reader.reads, 1);
    assert_eq!(found.skipped, Some(Skip::Cancelled));
}

#[test]
fn needs_scan_reflects_whether_content_must_be_read() {
    assert!(
        !Compiled::new(&FindQuery::parse("ext:rs"))
            .unwrap()
            .needs_scan()
    );
    assert!(
        Compiled::new(&FindQuery::parse("hello"))
            .unwrap()
            .needs_scan()
    );
    assert!(
        Compiled::new(&FindQuery::parse("re:foo"))
            .unwrap()
            .needs_scan()
    );
    assert!(
        Compiled::new(&FindQuery::parse("hex:4D5A"))
            .unwrap()
            .needs_scan()
    );
}
