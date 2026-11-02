use std::cell::Cell;

use proptest::prelude::*;

use super::{
    ByteSource, LOOK_BACK, LineIndex, MAX_ROW, previous_row, row_at, row_start_at, rows_down,
    rows_up,
};

struct Shrinking {
    bytes: Vec<u8>,
    len: Cell<u64>,
}

impl ByteSource for Shrinking {
    fn len(&self) -> u64 {
        self.len.get()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.len.set(1000);
        let len = usize::try_from(self.len.get()).unwrap();
        self.bytes[..len].read_at(offset, buf)
    }
}

struct Counting {
    bytes: Vec<u8>,
    read: Cell<u64>,
}

impl ByteSource for Counting {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let count = self.bytes.as_slice().read_at(offset, buf)?;
        self.read.set(self.read.get() + count as u64);
        Ok(count)
    }
}

#[test]
fn a_source_that_shrinks_while_stepping_does_not_hang() {
    let src = Shrinking {
        bytes: b"x\n".repeat(100_000),
        len: Cell::new(200_000),
    };
    assert!(row_start_at(&src, 150_000).is_ok());
}

#[test]
fn rows_inside_a_line_longer_than_the_look_back_step_by_whole_rows() {
    let bytes = vec![b'x'; 200_000];
    let src = bytes.as_slice();
    let last = row_start_at(src, 200_000).unwrap();
    assert!(200_000 - last <= MAX_ROW);
    assert!(200_000 - last > 1);
    let up = previous_row(src, last).unwrap();
    assert_eq!(last - up, MAX_ROW);
    assert_eq!(rows_down(src, up, 1).unwrap(), last);
    let start = row_start_at(src, 150_000).unwrap();
    assert!(150_000 - start < MAX_ROW);
    assert_eq!(previous_row(src, start).unwrap(), start - MAX_ROW);
}

#[test]
fn a_crlf_right_after_a_full_row_ends_that_row() {
    let mut bytes = vec![b'a'; 4096];
    bytes.extend_from_slice(b"\r\nb");
    let row = row_at(bytes.as_slice(), 0).unwrap();
    assert_eq!((row.end, row.next), (4096, 4098));
}

#[test]
fn line_numbers_in_one_huge_line_read_a_bounded_amount() {
    let src = Counting {
        bytes: vec![b'x'; 8 * 1024 * 1024],
        read: Cell::new(0),
    };
    let mut index = LineIndex::new();
    index.feed(&src.bytes);
    index.finish();
    assert_eq!(
        index.line_number(&src, 8 * 1024 * 1024 - 1).unwrap(),
        Some(1)
    );
    assert!(src.read.get() <= 1024 * 1024);
}

#[test]
fn a_line_start_after_a_huge_line_reads_a_bounded_amount() {
    let mut bytes = vec![b'x'; 8 * 1024 * 1024];
    bytes.extend_from_slice(b"\nnext\n");
    let src = Counting {
        bytes,
        read: Cell::new(0),
    };
    let mut index = LineIndex::new();
    index.feed(&src.bytes);
    index.finish();
    assert_eq!(
        index.line_start(&src, 2).unwrap(),
        Some(8 * 1024 * 1024 + 1)
    );
    assert!(src.read.get() <= 1024 * 1024);
}

#[test]
fn line_count_includes_a_last_line_without_a_newline() {
    let mut index = LineIndex::new();
    assert_eq!(index.line_count(b"".as_slice()).unwrap(), 0);
    index.feed(b"a\nb");
    assert_eq!(index.line_count(b"a\nb".as_slice()).unwrap(), 2);
    let mut ended = LineIndex::new();
    ended.feed(b"a\nb\n");
    assert_eq!(ended.line_count(b"a\nb\n".as_slice()).unwrap(), 2);
}

fn long_lines() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        (0usize..4, 1usize..200_000).prop_map(|(kind, len)| match kind {
            0 => vec![b'y'; len % 300],
            1 => "\u{e9}".repeat(len / 2).into_bytes(),
            2 => "a\u{6f22}".repeat(len / 4).into_bytes(),
            _ => vec![b'z'; len],
        }),
        1..4,
    )
    .prop_map(|lines| lines.join(&b'\n'))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn walking_down_then_up_past_the_look_back_returns_the_same_rows(bytes in long_lines()) {
        prop_assume!(bytes.len() as u64 > LOOK_BACK);
        let src = bytes.as_slice();
        let mut starts = vec![0u64];
        loop {
            let at = *starts.last().unwrap();
            let next = rows_down(src, at, 1).unwrap();
            if next == at {
                break;
            }
            starts.push(next);
        }
        for &at in &starts[1..] {
            let up = previous_row(src, at).unwrap();
            prop_assert!(up < at && at - up <= MAX_ROW + 2);
            prop_assert_eq!(rows_down(src, up, 1).unwrap(), at);
        }
        let last = *starts.last().unwrap();
        prop_assert_eq!(rows_up(src, last, bytes.len() as u64).unwrap(), 0);
    }
}

#[test]
fn up_inside_a_long_line_of_wide_characters_moves_a_whole_row() {
    let bytes = "\u{6f22}".repeat(100_000).into_bytes();
    let src = bytes.as_slice();
    let last = row_start_at(src, bytes.len() as u64).unwrap();
    assert!(bytes.len() as u64 - last >= MAX_ROW - 3);
    let up = previous_row(src, last).unwrap();
    assert!(last - up >= MAX_ROW - 3);
    assert_eq!(rows_down(src, up, 1).unwrap(), last);
    assert!(last - rows_up(src, last, 30).unwrap() >= 30 * (MAX_ROW - 3));
}

#[test]
fn a_tail_one_byte_over_a_row_is_cut() {
    let bytes = vec![b'a'; 4097];
    let row = row_at(bytes.as_slice(), 0).unwrap();
    assert_eq!((row.end, row.next), (4096, 4096));
}

#[test]
fn a_line_past_the_last_one_is_unknown_without_reading() {
    let src = Counting {
        bytes: vec![b'x'; 8 * 1024 * 1024],
        read: Cell::new(0),
    };
    let mut index = LineIndex::new();
    index.feed(&src.bytes[..6 * 1024 * 1024]);
    assert_eq!(index.line_start(&src, 2).unwrap(), None);
    index.feed(&src.bytes[6 * 1024 * 1024..]);
    index.finish();
    assert_eq!(index.line_start(&src, 2).unwrap(), None);
    assert_eq!(src.read.get(), 0);
}
