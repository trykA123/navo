use std::cell::{Cell, RefCell};
use std::io::{self, Write};

use navo_core::paging::ByteSource;

use super::{facts_of, lines, picks, utf16le_view, view};
use crate::text::diff::{Hunk, Row};
use crate::text::side::LineEnding;
use crate::text::{
    ApplyError, Direction, Picked, PickedHunk, TargetFacts, apply_hunk, apply_picked,
};

struct Short<'a> {
    bytes: &'a [u8],
    len: u64,
}

impl ByteSource for Short<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        <[u8] as ByteSource>::read_at(self.bytes, offset, buf)
    }
}

#[test]
fn a_short_source_read_is_an_error() {
    let target = lines(&[("a", LineEnding::Lf)]);
    let source = Short {
        bytes: b"x\n",
        len: 10,
    };
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..10,
        right_lines: 1..1,
        right_bytes: 2..2,
    };
    let mut out = Vec::new();
    let result = apply_hunk(
        &view(&target.bytes),
        &source,
        &hunk,
        Direction::LeftToRight,
        facts_of(&target.lines),
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Read(_))));
}

#[test]
fn a_short_target_read_is_an_error() {
    let target = Short {
        bytes: b"a\n",
        len: 10,
    };
    let source = lines(&[("x", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..2,
        right_lines: 0..1,
        right_bytes: 0..0,
    };
    let mut out = Vec::new();
    let result = apply_hunk(
        &target,
        &view(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Read(_))));
}

struct CountingSource<'a> {
    bytes: &'a [u8],
    largest: &'a Cell<usize>,
}

impl ByteSource for CountingSource<'_> {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.largest.set(self.largest.get().max(buf.len()));
        <[u8] as ByteSource>::read_at(self.bytes, offset, buf)
    }
}

struct ReadOnce<'a> {
    bytes: &'a [u8],
    seen: RefCell<Vec<bool>>,
}

impl ByteSource for ReadOnce<'_> {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let read = <[u8] as ByteSource>::read_at(self.bytes, offset, buf)?;
        let start = usize::try_from(offset).unwrap();
        let mut seen = self.seen.borrow_mut();
        for index in start..start + read {
            assert!(!seen[index], "a target byte was read twice");
            seen[index] = true;
        }
        Ok(read)
    }
}

struct Counter {
    bytes: u64,
}

impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes += u64::try_from(buf.len()).unwrap();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_20_mib_line_streams_with_bounded_buffers() {
    const LINE: usize = 20 * 1024 * 1024;
    let source = vec![b'x'; LINE];
    let target = b"a\n";
    let largest = Cell::new(0);
    let source = CountingSource {
        bytes: &source,
        largest: &largest,
    };
    let target = ReadOnce {
        bytes: target,
        seen: RefCell::new(vec![false; target.len()]),
    };
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..u64::try_from(LINE).unwrap(),
        right_lines: 1..1,
        right_bytes: 2..2,
    };
    let mut out = Counter { bytes: 0 };
    apply_hunk(
        &target,
        &source,
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert!(largest.get() <= 64 * 1024);
    assert_eq!(out.bytes, u64::try_from(LINE + 3).unwrap());
    assert!(target.seen.borrow().iter().all(|seen| *seen));
}

#[test]
fn a_crlf_across_the_buffer_boundary_is_copied_exactly() {
    let mut source = vec![b'x'; 65535];
    source.extend_from_slice(b"\r\n");
    let target = b"a\n";
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..65537,
        right_lines: 1..1,
        right_bytes: 2..2,
    };
    let mut out = Vec::new();
    apply_hunk(
        &view(target),
        &view(&source),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    let mut expected = b"a\n".to_vec();
    expected.extend(vec![b'x'; 65535]);
    expected.push(b'\n');
    assert_eq!(out, expected);
}

#[test]
fn a_utf16_surrogate_pair_across_the_buffer_boundary_is_copied_exactly() {
    let mut source = Vec::with_capacity(65_540);
    for _ in 0..32_767 {
        source.extend_from_slice(&0x0061u16.to_le_bytes());
    }
    source.extend_from_slice(&0xD83Du16.to_le_bytes());
    source.extend_from_slice(&0xDE00u16.to_le_bytes());
    source.extend_from_slice(&0x000Au16.to_le_bytes());
    let target = b"a\n";
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..65_540,
        right_lines: 1..1,
        right_bytes: 2..2,
    };
    let mut out = Vec::new();
    apply_hunk(
        &view(target),
        &utf16le_view(&source),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    let mut expected = b"a\n".to_vec();
    expected.extend(vec![b'a'; 32_767]);
    expected.extend_from_slice("\u{1F600}".as_bytes());
    expected.push(b'\n');
    assert_eq!(out, expected);
}

struct Sequential<'a> {
    bytes: &'a [u8],
    at: Cell<u64>,
    largest: Cell<usize>,
}

impl ByteSource for Sequential<'_> {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        assert_eq!(
            offset,
            self.at.get(),
            "reads must be monotonic and contiguous"
        );
        self.largest.set(self.largest.get().max(buf.len()));
        let read = <[u8] as ByteSource>::read_at(self.bytes, offset, buf)?;
        self.at.set(self.at.get() + u64::try_from(read).unwrap());
        Ok(read)
    }
}

#[test]
fn a_200_mib_target_reads_once_through_picked_insertions() {
    const LEN: usize = 200 * 1024 * 1024;
    const MID: u64 = 100 * 1024 * 1024;
    let mut target = Vec::with_capacity(LEN);
    while target.len() < LEN {
        target.extend_from_slice(&[b'x'; 63]);
        target.push(b'\n');
    }
    let source = lines(&[("new one", LineEnding::Lf), ("new two", LineEnding::Lf)]);
    let target = Sequential {
        bytes: &target,
        at: Cell::new(0),
        largest: Cell::new(0),
    };
    let line = usize::try_from(MID / 64).unwrap();
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: line..line,
        right_bytes: MID..MID,
    };
    let rows = [Row::Deleted { left: 0 }, Row::Deleted { left: 1 }];
    let picked = picks([0, 1]);
    let entries = [PickedHunk {
        hunk: &hunk,
        rows: &rows,
    }];
    let mut out = Counter { bytes: 0 };
    apply_picked(
        &target,
        &view(&source.bytes),
        Picked {
            facts: TargetFacts {
                dominant: LineEnding::Lf,
                final_newline: true,
            },
            hunks: &entries,
            left_starts: &source.starts,
            right_starts: &[],
            lines: &picked,
            direction: Direction::LeftToRight,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(target.at.get(), u64::try_from(LEN).unwrap());
    assert!(target.largest.get() <= 64 * 1024);
    assert_eq!(out.bytes, u64::try_from(LEN + source.bytes.len()).unwrap());
}
