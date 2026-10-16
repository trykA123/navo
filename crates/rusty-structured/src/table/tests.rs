use std::sync::atomic::AtomicBool;

use super::{
    Dialect, MAX_COLUMNS, ROWS_BYTES, RecordIndex, STRIDE, column_name, column_widths,
    index_records, parse_table, read_rows, sniff, sort_order,
};
use crate::Watch;

#[test]
fn sniffing_finds_comma_semicolon_tab_and_pipe_with_a_header() {
    for delimiter in [',', ';', '\t', '|'] {
        let text = format!(
            "city{d}people{d}area\nCluj{d}286598{d}179.5\nIasi{d}271692{d}93.9\nBrasov{d}237589{d}267.3\n",
            d = delimiter
        );
        let dialect = sniff(text.as_bytes());
        assert_eq!(dialect.delimiter, delimiter as u8, "{delimiter:?}");
        assert!(dialect.has_header, "{delimiter:?}");
    }
}

#[test]
fn quoted_fields_keep_delimiters_and_newlines() {
    let text = "name;note\n\"Smith; J\";\"line one\nline two\"\nDoe;plain\nPopa;\"a \"\"b\"\"\"\n";
    let dialect = sniff(text.as_bytes());
    assert_eq!(
        dialect,
        Dialect {
            delimiter: b';',
            has_header: true
        }
    );
    let table = parse_table(text.as_bytes(), dialect).unwrap();
    assert_eq!(table.header, ["name", "note"]);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0], ["Smith; J", "line one\nline two"]);
    assert_eq!(table.rows[2][1], "a \"b\"");
}

#[test]
fn a_numeric_first_row_is_data_and_columns_get_letters() {
    let dialect = sniff(b"1,2,3\n4,5,6\n7,8,9\n");
    assert!(!dialect.has_header);
    let table = parse_table(b"1,2,3\n4,5\n", dialect).unwrap();
    assert_eq!(table.header, ["A", "B", "C"]);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(column_name(26), "AA");
}

#[test]
fn numeric_columns_sort_by_value_and_text_without_case() {
    let rows: Vec<Vec<String>> = [
        ["10", "beta"],
        ["9", "Alpha"],
        ["", "gamma"],
        ["100", "alpha2"],
    ]
    .iter()
    .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
    .collect();
    assert_eq!(sort_order(&rows, 0, false), [1, 0, 3, 2]);
    assert_eq!(sort_order(&rows, 0, true), [3, 0, 1, 2]);
    assert_eq!(sort_order(&rows, 1, false), [1, 3, 0, 2]);
}

#[test]
fn widths_come_from_the_sample_and_are_clamped() {
    let header = vec!["id".to_owned(), "x".to_owned()];
    let rows = vec![vec!["1".to_owned(), "y".repeat(90)]];
    assert_eq!(column_widths(&header, &rows, 2), [4, 40]);
}

#[test]
fn the_record_index_reads_any_rows_on_demand() {
    let mut text = String::from("id,note\n");
    for row in 0..100 {
        if row % 7 == 0 {
            text.push_str(&format!("{row},\"multi\nline {row}\"\r\n"));
        } else {
            text.push_str(&format!("{row},plain {row}\n"));
        }
        if row % 30 == 0 {
            text.push('\n');
        }
    }
    let bytes = text.into_bytes();
    let dialect = sniff(&bytes);
    let table = parse_table(&bytes, dialect).unwrap();
    let mut read = |offset: u64, len: usize| {
        let start = offset as usize;
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    };
    let cancel = AtomicBool::new(false);
    let mut last = 0;
    let mut progress = |done: u64| last = done;
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let index = index_records(&mut read, bytes.len() as u64, dialect, &mut watch)
        .unwrap()
        .unwrap();
    assert_eq!(last, bytes.len() as u64);
    assert_eq!(index.records, 101);
    assert_eq!(index.starts.len() as u64, 101_u64.div_ceil(STRIDE));
    for (start, end) in [(1, 3), (14, 20), (0, 101), (95, 140), (32, 33)] {
        let rows = read_rows(&mut read, &index, dialect, start..end)
            .unwrap()
            .rows;
        let expected: Vec<Vec<String>> = (start..end.min(101))
            .map(|record| {
                if record == 0 {
                    table.header.clone()
                } else {
                    table.rows[record as usize - 1].clone()
                }
            })
            .collect();
        assert_eq!(rows, expected, "{start}..{end}");
    }
}

fn memory(bytes: &[u8]) -> impl FnMut(u64, usize) -> Result<Vec<u8>, String> + '_ {
    move |offset, len| {
        let start = usize::try_from(offset).unwrap().min(bytes.len());
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    }
}

fn index_of(bytes: &[u8], dialect: Dialect) -> RecordIndex {
    let cancel = AtomicBool::new(false);
    let mut progress = |_: u64| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    index_records(&mut memory(bytes), bytes.len() as u64, dialect, &mut watch)
        .unwrap()
        .unwrap()
}

fn matches_the_parse(text: &str, picks: &[std::ops::Range<u64>]) {
    let bytes = text.as_bytes();
    let dialect = Dialect {
        delimiter: b',',
        has_header: false,
    };
    let table = parse_table(bytes, dialect).unwrap();
    let index = index_of(bytes, dialect);
    assert_eq!(index.records, table.rows.len() as u64);
    for pick in picks {
        let rows = read_rows(&mut memory(bytes), &index, dialect, pick.clone())
            .unwrap()
            .rows;
        let expected = &table.rows[pick.start as usize..pick.end as usize];
        assert_eq!(rows, expected, "{pick:?}");
    }
}

#[test]
fn a_cr_only_file_indexes_every_line_as_a_record() {
    let text: String = (0..50).map(|row| format!("{row},name {row}\r")).collect();
    matches_the_parse(&text, &[0..2, 20..23, 47..50]);
}

#[test]
fn a_stray_quote_inside_a_field_does_not_swallow_the_rest() {
    let text: String = (0..50)
        .map(|row| format!("{row},a 5\" screen,{row}\n"))
        .collect();
    matches_the_parse(&text, &[0..1, 30..33, 49..50]);
}

#[test]
fn a_bom_is_skipped_by_the_index_and_the_rows() {
    matches_the_parse("\u{FEFF}id,name\n1,a\n", &[0..1, 1..2]);
}

#[test]
fn a_single_20_mb_row_is_indexed_and_reading_rows_stays_under_the_byte_cap() {
    let mut bytes = b"a,b\n".to_vec();
    bytes.resize(bytes.len() + 20 * 1024 * 1024, b'x');
    bytes.extend_from_slice(b",y\n1,2\n");
    let dialect = Dialect {
        delimiter: b',',
        has_header: true,
    };
    let index = index_of(&bytes, dialect);
    assert_eq!(index.records, 3);
    let mut largest = 0;
    let mut read = |offset: u64, len: usize| {
        largest = largest.max(len);
        memory(&bytes)(offset, len)
    };
    let slice = read_rows(&mut read, &index, dialect, 0..3).unwrap();
    assert!(largest <= ROWS_BYTES, "{largest}");
    assert_eq!(slice.rows.len(), 1, "a cut row is dropped");
    assert_eq!(slice.rows[0], ["a", "b"]);
}

#[test]
fn rows_wider_than_4096_columns_are_cut_and_the_true_width_is_kept() {
    let line = vec!["1"; 5000].join(",");
    let text = format!("{line}\n{line}\n");
    let dialect = Dialect {
        delimiter: b',',
        has_header: false,
    };
    let table = parse_table(text.as_bytes(), dialect).unwrap();
    assert_eq!((table.columns, table.widest), (MAX_COLUMNS, 5000));
    assert_eq!(table.header.len(), MAX_COLUMNS);
    assert_eq!(table.rows[0].len(), MAX_COLUMNS);
    let index = index_of(text.as_bytes(), dialect);
    let slice = read_rows(&mut memory(text.as_bytes()), &index, dialect, 0..2).unwrap();
    assert_eq!((slice.rows[0].len(), slice.widest), (MAX_COLUMNS, 5000));
}
