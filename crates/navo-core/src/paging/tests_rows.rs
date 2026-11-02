use proptest::prelude::*;

use super::tests_support::Short;
use super::{
    ByteSource, LOOK_BACK, MAX_ROW, Row, previous_row, row_at, row_start_at, rows_down, rows_up,
};

fn row(start: u64, end: u64, next: u64) -> Row {
    Row { start, end, next }
}

fn long_line(fill: &str, count: usize) -> Vec<u8> {
    let mut bytes = fill.repeat(count).into_bytes();
    bytes.push(b'\n');
    bytes
}

#[test]
fn row_at_ends_before_the_newline() {
    let src = b"abc\ndef\n".as_slice();
    assert_eq!(row_at(src, 0).unwrap(), row(0, 3, 4));
    assert_eq!(row_at(src, 4).unwrap(), row(4, 7, 8));
}

#[test]
fn row_at_last_row_without_newline_ends_at_len() {
    let src = b"abc\nde".as_slice();
    assert_eq!(row_at(src, 4).unwrap(), row(4, 6, 6));
}

#[test]
fn row_at_empty_line_is_an_empty_row() {
    let src = b"a\n\nb".as_slice();
    assert_eq!(row_at(src, 2).unwrap(), row(2, 2, 3));
}

#[test]
fn row_at_hides_carriage_return_before_newline() {
    let src = b"a\r\nb\r\n".as_slice();
    assert_eq!(row_at(src, 0).unwrap(), row(0, 1, 3));
    assert_eq!(row_at(src, 3).unwrap(), row(3, 4, 6));
}

#[test]
fn row_at_keeps_a_lone_carriage_return() {
    let src = b"a\rb\n".as_slice();
    assert_eq!(row_at(src, 0).unwrap(), row(0, 3, 4));
}

#[test]
fn row_at_past_the_end_is_empty_at_len() {
    let src = b"abc".as_slice();
    assert_eq!(row_at(src, 3).unwrap(), row(3, 3, 3));
    assert_eq!(row_at(src, 99).unwrap(), row(3, 3, 3));
}

#[test]
fn row_at_on_an_empty_source_is_empty() {
    assert_eq!(row_at(b"".as_slice(), 0).unwrap(), row(0, 0, 0));
}

#[test]
fn row_at_cuts_a_long_line_at_max_row() {
    let bytes = long_line("x", 5000);
    let src = bytes.as_slice();
    assert_eq!(row_at(src, 0).unwrap(), row(0, MAX_ROW, MAX_ROW));
    assert_eq!(row_at(src, MAX_ROW).unwrap(), row(MAX_ROW, 5000, 5001));
}

#[test]
fn row_at_keeps_a_line_of_exactly_max_row_whole() {
    let bytes = long_line("x", 4096);
    assert_eq!(row_at(bytes.as_slice(), 0).unwrap(), row(0, 4096, 4097));
}

#[test]
fn row_at_cut_on_an_even_two_byte_boundary_stays() {
    let bytes = long_line("é", 2500);
    assert_eq!(row_at(bytes.as_slice(), 0).unwrap(), row(0, 4096, 4096));
}

#[test]
fn row_at_never_splits_a_two_byte_char() {
    let mut bytes = b"a".to_vec();
    bytes.extend(long_line("é", 2500));
    let got = row_at(bytes.as_slice(), 0).unwrap();
    assert_eq!(got, row(0, 4095, 4095));
    assert!(std::str::from_utf8(&bytes[..4095]).is_ok());
}

#[test]
fn row_at_never_splits_a_three_byte_char() {
    let bytes = long_line("€", 2000);
    let got = row_at(bytes.as_slice(), 0).unwrap();
    assert_eq!(got, row(0, 4095, 4095));
    let next = row_at(bytes.as_slice(), got.next).unwrap();
    assert_eq!(next, row(4095, 6000, 6001));
}

#[test]
fn row_at_moves_a_cut_back_at_most_three_bytes() {
    let mut bytes = vec![0x80; 5000];
    bytes.push(b'\n');
    assert_eq!(row_at(bytes.as_slice(), 0).unwrap(), row(0, 4093, 4093));
}

#[test]
fn row_at_loops_over_short_reads() {
    let bytes = long_line("x", 5000);
    let src = Short::new(&bytes, 7);
    assert_eq!(row_at(&src, 0).unwrap(), row(0, MAX_ROW, MAX_ROW));
    assert_eq!(row_at(&src, MAX_ROW).unwrap(), row(MAX_ROW, 5000, 5001));
}

#[test]
fn row_start_at_finds_the_line_start() {
    let src = b"abc\ndef\n".as_slice();
    for offset in 0..4 {
        assert_eq!(row_start_at(src, offset).unwrap(), 0, "{offset}");
    }
    for offset in 4..8 {
        assert_eq!(row_start_at(src, offset).unwrap(), 4, "{offset}");
    }
}

#[test]
fn row_start_at_newline_belongs_to_the_row_it_ends() {
    let src = b"a\r\nb".as_slice();
    assert_eq!(row_start_at(src, 1).unwrap(), 0);
    assert_eq!(row_start_at(src, 2).unwrap(), 0);
    assert_eq!(row_start_at(src, 3).unwrap(), 3);
}

#[test]
fn row_start_at_past_the_end_gives_the_last_row() {
    let src = b"abc\ndef\n".as_slice();
    assert_eq!(row_start_at(src, 8).unwrap(), 4);
    assert_eq!(row_start_at(src, 1000).unwrap(), 4);
    assert_eq!(row_start_at(b"abc\nde".as_slice(), 6).unwrap(), 4);
}

#[test]
fn row_start_at_on_an_empty_source_is_zero() {
    assert_eq!(row_start_at(b"".as_slice(), 0).unwrap(), 0);
    assert_eq!(row_start_at(b"".as_slice(), 5).unwrap(), 0);
}

#[test]
fn row_start_at_in_a_long_line_matches_the_cuts() {
    let bytes = long_line("x", 10_000);
    let src = bytes.as_slice();
    assert_eq!(row_start_at(src, 4095).unwrap(), 0);
    assert_eq!(row_start_at(src, 5000).unwrap(), 4096);
    assert_eq!(row_start_at(src, 9000).unwrap(), 8192);
    assert_eq!(row_start_at(src, 10_000).unwrap(), 8192);
}

#[test]
fn row_start_at_in_a_multibyte_line_matches_the_cuts() {
    let mut bytes = b"a".to_vec();
    bytes.extend(long_line("é", 2500));
    let src = bytes.as_slice();
    assert_eq!(row_start_at(src, 4094).unwrap(), 0);
    assert_eq!(row_start_at(src, 4095).unwrap(), 4095);
    assert_eq!(row_start_at(src, 4500).unwrap(), 4095);
}

#[test]
fn row_start_at_beyond_look_back_stays_within_one_row() {
    let bytes = long_line("x", 200_000);
    let src = bytes.as_slice();
    let offset = 150_000;
    let start = row_start_at(src, offset).unwrap();
    assert!(start <= offset);
    assert!(offset - start < MAX_ROW);
    assert!(offset - start <= LOOK_BACK);
}

#[test]
fn row_start_at_loops_over_short_reads() {
    let bytes = long_line("x", 10_000);
    let src = Short::new(&bytes, 7);
    assert_eq!(row_start_at(&src, 9000).unwrap(), 8192);
}

#[test]
fn previous_row_at_zero_stays_at_zero() {
    assert_eq!(previous_row(b"abc\n".as_slice(), 0).unwrap(), 0);
}

#[test]
fn previous_row_goes_to_the_row_before() {
    let src = b"abc\ndef\nghi".as_slice();
    assert_eq!(previous_row(src, 4).unwrap(), 0);
    assert_eq!(previous_row(src, 8).unwrap(), 4);
}

#[test]
fn previous_row_steps_back_one_cut_in_a_long_line() {
    let bytes = long_line("x", 10_000);
    assert_eq!(previous_row(bytes.as_slice(), 8192).unwrap(), 4096);
}

#[test]
fn rows_down_advances_by_count() {
    let src = b"a\nb\nc\n".as_slice();
    assert_eq!(rows_down(src, 0, 0).unwrap(), 0);
    assert_eq!(rows_down(src, 0, 1).unwrap(), 2);
    assert_eq!(rows_down(src, 0, 2).unwrap(), 4);
}

#[test]
fn rows_down_stops_at_the_last_row_start() {
    let src = b"a\nb\nc\n".as_slice();
    assert_eq!(rows_down(src, 0, 10).unwrap(), 4);
    assert_eq!(rows_down(src, 4, 1).unwrap(), 4);
    assert_eq!(rows_down(b"a\nb".as_slice(), 0, 5).unwrap(), 2);
}

#[test]
fn rows_down_on_an_empty_source_stays_at_zero() {
    assert_eq!(rows_down(b"".as_slice(), 0, 3).unwrap(), 0);
}

#[test]
fn rows_down_steps_through_long_line_cuts() {
    let bytes = long_line("x", 10_000);
    assert_eq!(rows_down(bytes.as_slice(), 0, 2).unwrap(), 8192);
}

#[test]
fn rows_up_goes_back_by_count() {
    let src = b"a\nb\nc\n".as_slice();
    assert_eq!(rows_up(src, 4, 0).unwrap(), 4);
    assert_eq!(rows_up(src, 4, 1).unwrap(), 2);
    assert_eq!(rows_up(src, 4, 2).unwrap(), 0);
}

#[test]
fn rows_up_stops_at_zero() {
    let src = b"a\nb\nc\n".as_slice();
    assert_eq!(rows_up(src, 4, 10).unwrap(), 0);
    assert_eq!(rows_up(src, 0, 1).unwrap(), 0);
}

fn walk_down<S: ByteSource + ?Sized>(src: &S) -> Vec<u64> {
    let mut starts = vec![0];
    let mut at = 0;
    loop {
        let next = rows_down(src, at, 1).unwrap();
        if next == at {
            return starts;
        }
        assert!(next > at && next < src.len());
        starts.push(next);
        at = next;
    }
}

fn walk_up<S: ByteSource + ?Sized>(src: &S, last: u64) -> Vec<u64> {
    let mut starts = vec![last];
    let mut at = last;
    while at > 0 {
        let previous = rows_up(src, at, 1).unwrap();
        assert!(previous < at);
        starts.push(previous);
        at = previous;
    }
    starts
}

fn text() -> impl Strategy<Value = Vec<u8>> {
    let short = "[a-z ]{0,40}".prop_map(String::into_bytes);
    let long = (4000usize..12_000).prop_map(|len| vec![b'x'; len]);
    let line = prop_oneof![4 => short, 1 => long];
    (prop::collection::vec(line, 0..12), any::<bool>()).prop_map(|(lines, trailing)| {
        let mut bytes = lines.join(&b'\n');
        if trailing {
            bytes.push(b'\n');
        }
        bytes
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn walking_down_then_up_visits_the_same_starts(bytes in text()) {
        let src = bytes.as_slice();
        let down = walk_down(src);
        let mut up = walk_up(src, *down.last().unwrap());
        up.reverse();
        prop_assert_eq!(down, up);
    }

    #[test]
    fn row_start_at_any_offset_is_a_walked_start(
        bytes in text(),
        picks in prop::collection::vec(any::<prop::sample::Index>(), 8),
    ) {
        let src = bytes.as_slice();
        let starts = walk_down(src);
        for pick in picks {
            let offset = pick.index(bytes.len() + 2) as u64;
            let start = row_start_at(src, offset).unwrap();
            prop_assert!(starts.contains(&start), "offset {} gave {}", offset, start);
        }
    }

    #[test]
    fn row_at_next_matches_rows_down(bytes in text()) {
        let src = bytes.as_slice();
        let starts = walk_down(src);
        for pair in starts.windows(2) {
            prop_assert_eq!(row_at(src, pair[0]).unwrap().next, pair[1]);
        }
    }
}
