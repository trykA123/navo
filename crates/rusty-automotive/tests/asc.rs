mod support;

use rusty_automotive::{Base, Body, CanId, Direction, Evidence, Frame, FrameKind, sniff_trace};
use support::{entries_of, every_entry, frame, trace_more, trace_of};

const HEX_LOG: &str = "\
date Mon Oct 19 09:41:07.000 am 2026\r
base hex  timestamps absolute\r
internal events logged\r
// version 13.0.0\r
Begin Triggerblock Mon Oct 19 09:41:07.000 am 2026\r
   0.000000 Start of measurement\r
   0.010000 1  123             Rx   d 8 01 02 03 04 05 06 07 08  Length = 230000 BitCount = 119 ID = 291\r
   0.020000 2  18FEF100x       Tx   d 3 AA BB CC\r
   0.030000 1  123             Rx   r\r
   0.035000 1  7DF             Rx   r 8\r
   0.040000 1  ErrorFrame\r
   0.050000 CANFD   1 Rx        1A0  EngineData                      1 0 9 12 00 11 22 33 44 55 66 77 88 99 AA BB   130000  0 1000 0 0 0 0 0\r
   0.060000 CANFD   2 Tx   18DA00F1x                                 0 1 2 2 10 20   0 0 0 0 0 0 0 0\r
   0.070000 1  123             Rx   d 8 11 12 13 14 15 16 17 18\r
End TriggerBlock\r
";

fn std_id(raw: u32) -> Option<CanId> {
    Some(CanId {
        raw,
        extended: false,
    })
}

#[test]
fn headers_are_read_and_are_not_rows() {
    let trace = trace_of(HEX_LOG);
    assert_eq!(
        trace.date.as_deref(),
        Some("Mon Oct 19 09:41:07.000 am 2026")
    );
    assert_eq!(trace.notation().base, Base::Hex);
    assert!(!trace.notation().relative);
    assert_eq!(trace.internal_events, Some(true));
    assert_eq!(trace.rows(), 9);
    assert_eq!(trace.lines, 15);
}

#[test]
fn a_classic_frame_keeps_its_fields_and_ignores_trailing_ones() {
    let trace = trace_of(HEX_LOG);
    let entries = entries_of(HEX_LOG, &trace, &[1]);
    assert_eq!(entries[0].time_ns, Some(10_000_000));
    assert_eq!(entries[0].line, 7);
    assert_eq!(
        frame(&entries[0]),
        &Frame {
            channel: 1,
            id: std_id(0x123),
            direction: Some(Direction::Rx),
            kind: FrameKind::Can,
            dlc: 8,
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }
    );
}

#[test]
fn an_x_suffix_marks_an_extended_id() {
    let trace = trace_of(HEX_LOG);
    let frame = frame(&entries_of(HEX_LOG, &trace, &[2])[0]).clone();
    assert_eq!(
        frame.id,
        Some(CanId {
            raw: 0x18FE_F100,
            extended: true
        })
    );
    assert_eq!(frame.direction, Some(Direction::Tx));
    assert_eq!(frame.data, vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn remote_frames_have_no_data_and_an_optional_dlc() {
    let trace = trace_of(HEX_LOG);
    let entries = entries_of(HEX_LOG, &trace, &[3, 4]);
    assert_eq!(frame(&entries[0]).kind, FrameKind::Remote);
    assert_eq!(frame(&entries[0]).dlc, 0);
    assert_eq!(frame(&entries[1]).dlc, 8);
    assert!(frame(&entries[1]).data.is_empty());
}

#[test]
fn an_error_frame_has_a_channel_and_no_id() {
    let trace = trace_of(HEX_LOG);
    let error = frame(&entries_of(HEX_LOG, &trace, &[5])[0]).clone();
    assert_eq!(error.kind, FrameKind::Error);
    assert_eq!(error.channel, 1);
    assert_eq!(error.id, None);
    assert_eq!(trace.kinds.error, 1);
}

#[test]
fn can_fd_lines_read_flags_length_and_skip_a_symbolic_name() {
    let trace = trace_of(HEX_LOG);
    let entries = entries_of(HEX_LOG, &trace, &[6, 7]);
    let named = frame(&entries[0]);
    assert_eq!(
        named.kind,
        FrameKind::Fd {
            brs: true,
            esi: false
        }
    );
    assert_eq!(named.id, std_id(0x1A0));
    assert_eq!(named.dlc, 9);
    assert_eq!(named.data.len(), 12);
    assert_eq!(named.data[11], 0xBB);
    let plain = frame(&entries[1]);
    assert_eq!(
        plain.kind,
        FrameKind::Fd {
            brs: false,
            esi: true
        }
    );
    assert_eq!(plain.channel, 2);
    assert_eq!(plain.data, vec![0x10, 0x20]);
}

#[test]
fn unknown_lines_are_kept_as_other_rows() {
    let trace = trace_of(HEX_LOG);
    let entries = entries_of(HEX_LOG, &trace, &[0]);
    assert_eq!(
        entries[0].body,
        Body::Other("0.000000 Start of measurement".to_owned())
    );
    assert_eq!(entries[0].time_ns, Some(0));
    assert_eq!(trace.kinds.other, 1);
    assert_eq!(trace.malformed, 0);
}

#[test]
fn ids_are_counted_per_channel_with_their_mean_period() {
    let trace = trace_of(HEX_LOG);
    let stat = trace
        .stats
        .iter()
        .find(|stat| stat.id == std_id(0x123).unwrap() && stat.channel == 1)
        .unwrap();
    assert_eq!(stat.count, 3);
    assert_eq!(stat.mean_period_ns(), Some(30_000_000));
    assert_eq!(trace.stats.len(), 5);
    assert_eq!(trace.channels, vec![1, 2]);
    assert_eq!(trace.kinds.frames(), 8);
    assert_eq!(trace.duration_ns(), 70_000_000);
}

#[test]
fn rows_with_an_id_and_the_previous_one_are_found_from_the_index() {
    let trace = trace_of(HEX_LOG);
    let key = trace.key_of(1).unwrap();
    assert_eq!(trace.rows_with(key), vec![1, 3, 8]);
    assert_eq!(trace.previous_with(8), Some(3));
    assert_eq!(trace.previous_with(1), None);
    assert_eq!(trace.key_of(5), None, "error frames have no ID");
}

#[test]
fn decimal_base_reads_ids_and_bytes_as_decimal() {
    let text = "base dec  timestamps absolute\n1.5 1 291 Rx d 2 255 16\n1.6 1 419430400x Rx d 0\n";
    let trace = trace_of(text);
    let entries = every_entry(text, &trace);
    assert_eq!(frame(&entries[0]).id, std_id(291));
    assert_eq!(frame(&entries[0]).data, vec![255, 16]);
    assert_eq!(frame(&entries[1]).id.unwrap().raw, 0x1900_0000);
    assert_eq!(entries[0].time_ns, Some(1_500_000_000));
}

#[test]
fn relative_timestamps_add_up() {
    let text = "base hex  timestamps relative\n0.1 1 10 Rx d 0\n0.25 1 10 Rx d 0\n0.05 Start of measurement\n";
    let trace = trace_of(text);
    let times: Vec<_> = every_entry(text, &trace)
        .iter()
        .map(|entry| entry.time_ns)
        .collect();
    assert_eq!(
        times,
        vec![Some(100_000_000), Some(350_000_000), Some(400_000_000)]
    );
    assert_eq!(trace.stats[0].mean_period_ns(), Some(250_000_000));
}

#[test]
fn relative_time_survives_decoding_from_a_later_block() {
    let mut text = String::from("base hex  timestamps relative\n");
    for _ in 0..100 {
        text.push_str("0.001 1 10 Rx d 1 FF\n");
    }
    let trace = trace_of(&text);
    let entry = &entries_of(&text, &trace, &[99])[0];
    assert_eq!(entry.time_ns, Some(100_000_000));
    assert_eq!(entry.line, 101);
}

#[test]
fn a_frame_with_missing_bytes_is_malformed_with_its_line() {
    let text = "base hex  timestamps absolute\n0.1 1 123 Rx d 8 01 02\n0.2 1 123 Rx d 1 ZZ\n0.3 1 FFFF Rx d 0\n0.4 1 123 Rx d 1 01\n";
    let trace = trace_of(text);
    assert_eq!(trace.malformed, 3);
    assert_eq!(trace.problems[0].line, 2);
    assert_eq!(trace.rows(), 4);
    let entries = every_entry(text, &trace);
    assert!(
        matches!(&entries[0].body, Body::Malformed { text, .. } if text == "0.1 1 123 Rx d 8 01 02")
    );
    assert!(matches!(entries[2].body, Body::Malformed { reason, .. } if reason.contains("ID")));
    assert_eq!(frame(&entries[3]).data, vec![1]);
}

#[test]
fn a_line_longer_than_four_kib_is_skipped_as_malformed() {
    let long = "9".repeat(5000);
    let text = format!("0.1 1 10 Rx d 0\n0.2 1 {long} Rx d 0\n0.3 1 11 Rx d 0\n");
    let trace = trace_of(&text);
    assert_eq!(trace.rows(), 2);
    assert_eq!(trace.malformed, 1);
    assert_eq!(trace.problems[0].line, 2);
    assert!(trace.problems[0].reason.contains("4 KiB"));
    let entries = every_entry(&text, &trace);
    assert_eq!(frame(&entries[1]).id, std_id(0x11));
    assert_eq!(entries[1].line, 3);
}

#[test]
fn an_unterminated_last_line_still_counts_when_the_whole_file_is_read() {
    let trace = trace_of("0.1 1 10 Rx d 0\n0.2 1 11 Rx d 0");
    assert_eq!(trace.rows(), 2);
}

#[test]
fn reading_more_appends_only_complete_new_lines() {
    let first = "base hex  timestamps absolute\n0.1 1 10 Rx d 1 01\n";
    let mut trace = trace_of(first);
    let grown = format!("{first}0.2 1 10 Rx d 1 02\n0.3 1 2");
    let more = trace_more(&mut trace, &grown);
    assert_eq!(more, 1);
    assert_eq!(trace.rows(), 2);
    let full = format!("{grown}0 Rx d 1 03\n");
    assert_eq!(trace_more(&mut trace, &full), 1);
    assert_eq!(trace.rows(), 3);
    assert_eq!(trace.stats.len(), 2);
    assert_eq!(trace.stats[0].count, 2);
    let entries = every_entry(&full, &trace);
    assert_eq!(frame(&entries[2]).id, std_id(0x20));
    assert_eq!(trace.rows_with(trace.key_of(0).unwrap()), vec![0, 1]);
}

#[test]
fn a_header_or_frame_lines_sniff_as_a_trace() {
    assert_eq!(sniff_trace(HEX_LOG.as_bytes()), Some(Evidence::Header));
    let frames = "   0.010000 1  123  Rx   d 1 01\n   0.020000 1  124  Rx   d 1 02\n";
    assert_eq!(sniff_trace(frames.as_bytes()), Some(Evidence::Frames));
}

#[test]
fn armoured_keys_and_prose_do_not_sniff_as_a_trace() {
    let key =
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nmQINBGB7\n-----END PGP PUBLIC KEY BLOCK-----\n";
    assert_eq!(sniff_trace(key.as_bytes()), None);
    assert_eq!(sniff_trace(b"1 apple\n2 pears\n"), None);
}

#[test]
fn reading_in_chunks_leaves_an_unterminated_last_line_for_later() {
    let text = "0.1 1 10 Rx d 1 01\n0.2 1 11 Rx d 8 01 02";
    let trace = support::chunked_trace(text, 1000);
    assert_eq!(trace.rows(), 1);
    assert_eq!(trace.end, 19);
}

#[test]
fn more_lines_after_a_chunked_read_reuse_the_known_ids() {
    let first = "0.1 1 10 Rx d 1 01\n0.2 1 11 Rx d 1 02\n";
    let mut trace = support::chunked_trace(first, 1);
    let grown = format!("{first}0.3 1 10 Rx d 1 03\n");
    assert_eq!(trace_more(&mut trace, &grown), 1);
    assert_eq!(trace.stats.len(), 2);
    assert_eq!(trace.stats[0].count, 2);
    assert_eq!(trace.key_of(2), trace.key_of(0));
}

#[test]
fn ids_beyond_the_limit_are_counted_but_not_listed() {
    let mut text = String::new();
    for raw in 0..=rusty_automotive::MAX_IDS as u32 {
        text.push_str(&format!("0.1 1 {raw:X}x Rx d 0\n"));
    }
    let trace = trace_of(&text);
    assert_eq!(trace.stats.len(), rusty_automotive::MAX_IDS);
    assert_eq!(trace.unlisted, 1);
    assert_eq!(trace.key_of(rusty_automotive::MAX_IDS as u64), None);
    assert_eq!(trace.kinds.can, rusty_automotive::MAX_IDS as u64 + 1);
}

#[test]
fn a_byte_order_mark_does_not_hide_the_header() {
    let text = "\u{FEFF}base dec  timestamps relative\n0.1 1 10 Rx d 0\n";
    let trace = trace_of(text);
    assert_eq!(trace.notation().base, Base::Dec);
    assert!(trace.notation().relative);
    assert_eq!(trace.rows(), 1);
}

#[test]
fn a_can_fd_dlc_is_read_in_the_file_base() {
    let text = "base dec  timestamps absolute\n0.1 CANFD 1 Rx 291 1 0 13 32 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31\n";
    let trace = trace_of(text);
    let frame = frame(&every_entry(text, &trace)[0]).clone();
    assert_eq!(frame.dlc, 13);
    assert_eq!(frame.data.len(), 32);
}
