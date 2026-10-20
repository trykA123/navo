mod support;

use rusty_automotive::{
    Body, CanId, Direction, Evidence, FrameKind, TRACE_EXTENSIONS, sniff_trace,
};
use support::{entries_of, every_entry, frame, trace_more, trace_of};

#[test]
fn trc_11_keeps_classic_remote_and_error_frames() {
    let text = ";$FILEVERSION=1.1\r\n;$STARTTIME=45000.5\r\n; invented trace\r\n1) 1.2 Rx 0010 2 01 02\r\n2) 2.3 Tx 00000010 4 RTR\r\n3) 3.4 Error 0001 4 00 02 03 04\r\n4) 4.5 Warng FFFFFFFF 4 00 00 00 04 BUSLIGHT\r\n";
    let trace = trace_of(text);
    let entries = every_entry(text, &trace);
    assert_eq!(trace.date.as_deref(), Some("45000.5"));
    assert_eq!(trace.rows(), 4);
    assert_eq!(entries[0].time_ns, Some(1_200_000));
    assert_eq!(entries[0].line, 4);
    assert_eq!(frame(&entries[0]).channel, 1);
    assert_eq!(frame(&entries[0]).data, vec![1, 2]);
    assert_eq!(frame(&entries[1]).kind, FrameKind::Remote);
    assert_eq!(frame(&entries[1]).dlc, 4);
    assert_eq!(frame(&entries[1]).direction, Some(Direction::Tx));
    assert_eq!(
        frame(&entries[1]).id,
        Some(CanId {
            raw: 16,
            extended: true
        })
    );
    assert_eq!(frame(&entries[2]).kind, FrameKind::Error);
    assert_eq!(frame(&entries[2]).id, None);
    assert_eq!(frame(&entries[2]).data, vec![0, 2, 3, 4]);
    assert!(matches!(entries[3].body, Body::Other(_)));
    assert_eq!(trace.kinds.other, 1);
    assert_eq!(trace.malformed, 0);
}

#[test]
fn trc_13_keeps_bus_and_skips_the_reserved_column() {
    let text = ";$FILEVERSION=1.3\n;$STARTTIME=45000\n1) 123.456 3 Rx 0020 - 1 11\n2) 125.456 3 Tx 0020 - 0\n3) 126.000 2 Error 0002 - 4 01 00 00 00\n";
    let trace = trace_of(text);
    let entries = every_entry(text, &trace);
    assert_eq!(trace.channels, vec![2, 3]);
    assert_eq!(entries[0].time_ns, Some(123_456_000));
    assert_eq!(frame(&entries[0]).channel, 3);
    assert_eq!(frame(&entries[0]).id.unwrap().raw, 32);
    assert_eq!(frame(&entries[0]).data, vec![17]);
    assert_eq!(frame(&entries[1]).direction, Some(Direction::Tx));
    assert_eq!(frame(&entries[2]).kind, FrameKind::Error);
    assert_eq!(trace.stats[0].count, 2);
    assert_eq!(trace.stats[0].mean_period_ns(), Some(2_000_000));
}

#[test]
fn trc_20_reads_length_and_omitted_event_fields() {
    let text = ";$FILEVERSION=2.0\n;$COLUMNS=N,O,T,I,d,l,D\n1 1.001 DT 0010 Rx 1 01\n2 2.002 FB 0020 Tx 12 00 01 02 03 04 05 06 07 08 09 0A 0B\n3 3.003 ER Rx 04 00 02 00 00\n4 4.004 ST Rx 00 00 00 08\n5 5.005 EC Rx 02 02\n6 6.006 RR 0010 Rx 8\n";
    let trace = trace_of(text);
    let entries = every_entry(text, &trace);
    assert_eq!(entries[0].time_ns, Some(1_001_000));
    assert_eq!(
        frame(&entries[1]).kind,
        FrameKind::Fd {
            brs: true,
            esi: false
        }
    );
    assert_eq!(frame(&entries[1]).dlc, 9);
    assert_eq!(frame(&entries[1]).data.len(), 12);
    assert_eq!(frame(&entries[2]).kind, FrameKind::Error);
    assert_eq!(frame(&entries[2]).data, vec![4, 0, 2, 0, 0]);
    assert!(matches!(entries[3].body, Body::Other(_)));
    assert!(matches!(entries[4].body, Body::Other(_)));
    assert_eq!(frame(&entries[5]).kind, FrameKind::Remote);
    assert_eq!(trace.kinds.other, 2);
    assert_eq!(trace.malformed, 0);
}

#[test]
fn trc_21_reads_dlc_bus_reserved_and_all_fd_flags() {
    let mut text = String::from(";$FILEVERSION=2.1\n;$COLUMNS=N,O,T,B,I,d,R,L,D\n");
    for (at, kind) in ["FD", "FB", "FE", "BI"].iter().enumerate() {
        text.push_str(&format!(
            "{} 1.000 {kind} 2 00000010 Tx - 9 00 01 02 03 04 05 06 07 08 09 0A 0B\n",
            at + 1
        ));
    }
    text.push_str("5 2.000 ER 2 - Rx - 5 04 01 02 03 04\n6 3.000 EV - invented event\n7 4.000 ST 2 - Rx - 4 00 00 00 08\n8 5.000 EC 2 - Rx - 2 03 04\n");
    let trace = trace_of(&text);
    let entries = every_entry(&text, &trace);
    for (at, entry) in entries[..4].iter().enumerate() {
        let frame = frame(entry);
        assert_eq!(
            frame.kind,
            FrameKind::Fd {
                brs: at % 2 == 1,
                esi: at >= 2
            }
        );
        assert_eq!(frame.channel, 2);
        assert_eq!(frame.dlc, 9);
        assert_eq!(frame.data.len(), 12);
        assert_eq!(
            frame.id,
            Some(CanId {
                raw: 16,
                extended: true
            })
        );
    }
    assert_eq!(frame(&entries[4]).kind, FrameKind::Error);
    assert!(matches!(entries[5].body, Body::Other(_)));
    assert_eq!(trace.kinds.fd, 4);
    assert_eq!(trace.kinds.error, 1);
    assert_eq!(trace.kinds.other, 3);
    assert_eq!(trace.malformed, 0);
}

#[test]
fn optional_trc_columns_and_an_empty_fd_payload_decode() {
    let text = ";$FILEVERSION=2.1\n;$COLUMNS=O,T,I,d,L,D\n1.234567 FD 0010 Rx 0\n";
    let entries = every_entry(text, &trace_of(text));
    assert_eq!(entries[0].time_ns, Some(1_234_567));
    assert_eq!(frame(&entries[0]).channel, 1);
    assert_eq!(frame(&entries[0]).dlc, 0);
    assert!(frame(&entries[0]).data.is_empty());
}

#[test]
fn malformed_trc_rows_report_their_physical_line_and_keep_later_frames() {
    let text = ";$FILEVERSION=2.1\n;$COLUMNS=N,O,T,B,I,d,L,D\n1 no DT 1 0010 Rx 0\n2 1 DT 0 0010 Rx 0\n3 1 DT 1 0010 Wrong 0\n4 1 DT 1 9999 Rx 0\n5 1 FD 1 0010 Rx 16\n6 1 DT 1 0010 Rx 2 01\n7 1 DT 1 0010 Rx 1 ZZ\n8 1 DT 1 0010 Rx 1 01\n";
    let trace = trace_of(text);
    assert_eq!(trace.malformed, 7);
    assert_eq!(
        trace.problems.iter().map(|p| p.line).collect::<Vec<_>>(),
        (3..=9).collect::<Vec<_>>()
    );
    let entries = every_entry(text, &trace);
    assert!(
        entries[..7]
            .iter()
            .all(|entry| matches!(entry.body, Body::Malformed { .. }))
    );
    assert_eq!(entries[7].line, 10);
    assert_eq!(frame(&entries[7]).data, vec![1]);
}

#[test]
fn malformed_trc_headers_and_missing_columns_are_reported() {
    for header in [
        ";$FILEVERSION=3.0",
        ";$FILEVERSION",
        ";$STARTTIME=NaN",
        ";$COLUMNS=N,O,T,D",
    ] {
        let text = format!(";$FILEVERSION=2.1\n{header}\n1 1 DT 0010 Rx 0\n");
        let trace = trace_of(&text);
        assert_eq!(trace.problems[0].line, 2);
        assert_eq!(trace.malformed, 2);
        assert_eq!(every_entry(&text, &trace).len(), 2);
    }
}

#[test]
fn trc_follow_retains_headers_across_chunks_and_partial_lines() {
    let first = ";$FILEVERSION=2.1\n;$COLUMNS=O,T,B,I,d,l,D\n1.0 DT 2 0010 Rx 1 01\n";
    let mut trace = support::chunked_trace(first, 1);
    let partial = format!("{first}2.0 FE 2 0010 Tx 1");
    assert_eq!(trace_more(&mut trace, &partial), 0);
    let grown = format!("{partial} 02\n3.0 DT 2 0010 Rx 1 03\n");
    assert_eq!(trace_more(&mut trace, &grown), 2);
    assert_eq!(trace.stats[0].count, 3);
    assert_eq!(trace.rows_with(trace.key_of(0).unwrap()), vec![0, 1, 2]);
    let entries = every_entry(&grown, &trace);
    assert_eq!(entries[2].time_ns, Some(3_000_000));
    assert_eq!(
        frame(&entries[1]).kind,
        FrameKind::Fd {
            brs: false,
            esi: true
        }
    );
    assert_eq!(frame(&entries[2]).data, vec![3]);
}

#[test]
fn a_truncated_final_trc_row_is_reported_without_panicking() {
    let text = ";$FILEVERSION=1.1\n1) 1.0 Rx 0010 1 01\n2) 2.0 Rx 0010 8 01";
    let trace = trace_of(text);
    assert_eq!(trace.kinds.can, 1);
    assert_eq!(trace.problems[0].line, 3);
    assert!(matches!(
        every_entry(text, &trace)[1].body,
        Body::Malformed { .. }
    ));
}

#[test]
fn a_trc_window_reads_only_its_sparse_index_block() {
    let mut text = String::from(";$FILEVERSION=2.1\n;$COLUMNS=O,T,I,d,L,D\n");
    for _ in 0..10_000 {
        text.push_str("1.0 DT 0010 Rx 1 01\n");
    }
    let trace = trace_of(&text);
    let expected = entries_of(&text, &trace, &[6000]);
    let mut touched = Vec::new();
    let decoded = trace
        .plan(&[6000])
        .decode(&mut |at, len| {
            touched.push((at, len));
            let end = (at as usize + len).min(text.len());
            Ok(text.as_bytes()[at as usize..end].to_vec())
        })
        .unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].line, 6003);
    assert!(touched.iter().all(|(at, _)| *at > 100_000));
    assert!(touched.iter().map(|(_, len)| len).sum::<usize>() <= 32 * 32);
}

#[test]
fn trc_content_sniffs_with_a_bom_and_other_files_stay_out() {
    assert!(TRACE_EXTENSIONS.contains(&"trc"));
    for marker in [
        ";$FILEVERSION=1.1",
        ";$STARTTIME=45000",
        ";$COLUMNS=O,T,I,d,L,D",
    ] {
        assert_eq!(
            sniff_trace(format!("\u{feff}; comment\n  {marker}\r\n").as_bytes()),
            Some(Evidence::Header)
        );
    }
    assert_eq!(sniff_trace(b"this is a plain trc file\n"), None);
    assert_eq!(sniff_trace(b"\0\x01\x02\x03"), None);
}

#[test]
fn trc_layout_changes_survive_sparse_blocks() {
    let mut text = String::from("; invented preamble\n;$FILEVERSION=1.1\n");
    for _ in 0..70 {
        text.push_str("1) 1.0 Rx 0010 1 01\n");
    }
    text.push_str(";$FILEVERSION=2.1\n;$COLUMNS=O,T,B,I,d,l,D\n");
    for _ in 0..100 {
        text.push_str("2.0 BI 3 00000010 Tx 1 02\n");
    }
    let whole = trace_of(&text);
    let chunked = support::chunked_trace(&text, 13);
    assert_eq!(whole.rows(), 170);
    assert_eq!(every_entry(&text, &whole), every_entry(&text, &chunked));
    let entries = entries_of(&text, &whole, &[69, 70, 169]);
    assert_eq!(frame(&entries[0]).kind, FrameKind::Can);
    assert_eq!(
        frame(&entries[1]).kind,
        FrameKind::Fd {
            brs: true,
            esi: true
        }
    );
    assert_eq!(frame(&entries[2]).channel, 3);
    assert_eq!(entries[2].time_ns, Some(2_000_000));
}

#[test]
fn invalid_trc_bytes_lengths_and_overflows_are_reported() {
    let rows = [
        "1.0 DT +010 Rx 0",
        "1.0 DT 0010 Rx 1 +A",
        "1.0 DT 0010 Rx 9",
        "1.0 FD 0010 Rx 9",
        "1.0 DT 0010 Rx 0 01",
        "1.0 ER - Rx 4 01 02 03 04",
        "1.0 ER - Rx 5 01 02",
        "1.0 ST - Rx 4 01",
        "1.0 EC - Rx 2 ZZ 01",
        "18446744073709551615 DT 0010 Rx 0",
        "-1.0 DT 0010 Rx 0",
    ];
    for row in rows {
        let text = format!(";$FILEVERSION=2.1\n;$COLUMNS=O,T,I,d,l,D\n{row}\n");
        let trace = trace_of(&text);
        assert_eq!(trace.malformed, 1, "{row}");
        assert_eq!(trace.problems[0].line, 3, "{row}");
        assert!(
            matches!(every_entry(&text, &trace)[0].body, Body::Malformed { .. }),
            "{row}"
        );
    }
}

#[test]
fn non_utf8_trc_data_and_oversized_lines_do_not_panic() {
    use rusty_automotive::{Watch, index_trace};
    use std::sync::atomic::AtomicBool;
    let mut bytes = b";$FILEVERSION=1.1\n; comment \xff\n1) 1.0 Rx 0010 1 \xff\n".to_vec();
    bytes.extend(std::iter::repeat_n(b'9', 5000));
    bytes.extend(b"\n2) 2.0 Rx 0010 0\n");
    let trace = index_trace(
        bytes.as_slice(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap();
    assert_eq!(trace.malformed, 2);
    assert_eq!(trace.problems[0].line, 3);
    assert_eq!(trace.problems[1].line, 4);
    assert_eq!(trace.kinds.can, 1);
    let entries = trace
        .plan(&[0, 1])
        .decode(
            &mut |at, len| Ok(bytes[at as usize..(at as usize + len).min(bytes.len())].to_vec()),
        )
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[0].body, Body::Malformed { .. }));
    assert_eq!(entries[1].line, 5);
}
