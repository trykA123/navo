mod blf_build;

use blf_build::*;
use rusty_automotive::{
    CanId, Direction, Evidence, FrameKind, TRACE_EXTENSIONS, sniff_blf, sniff_trace,
};

#[test]
fn blf_maps_each_can_type_and_both_timestamp_headers() {
    for compressed in [false, true] {
        let mut inner = object(1, 1, 100, &classic(1, 0, 16));
        inner.extend(object(86, 2, 200, &classic(86, 129, (1 << 31) | 16)));
        inner.extend(object(100, 1, 300, &fd(100, 7)));
        inner.extend(object(101, 2, 400, &fd(101, 7 << 12)));
        inner.extend(object(73, 1, 500, &error()));
        let bytes = file(&[container(&inner, compressed)], 5);
        let trace = trace(&bytes);
        let entries = entries(&bytes, &trace, &[0, 1, 2, 3, 4]);
        assert_eq!(trace.rows(), 5);
        assert_eq!(trace.malformed, 0);
        assert_eq!(trace.kinds.can, 1);
        assert_eq!(trace.kinds.remote, 1);
        assert_eq!(trace.kinds.fd, 2);
        assert_eq!(trace.kinds.error, 1);
        assert_eq!(trace.channels, vec![2, 3, 4, 5]);
        assert_eq!(entries[0].time_ns, Some(100));
        assert_eq!(entries[1].time_ns, Some(2_000_000));
        assert_eq!(entries[3].time_ns, Some(4_000_000));
        assert_eq!(frame(&entries[0]).data, (0..8).collect::<Vec<u8>>());
        assert_eq!(frame(&entries[0]).direction, Some(Direction::Rx));
        assert_eq!(frame(&entries[1]).kind, FrameKind::Remote);
        assert_eq!(frame(&entries[1]).direction, Some(Direction::Tx));
        assert_eq!(frame(&entries[1]).dlc, 8);
        assert!(frame(&entries[1]).data.is_empty());
        assert_eq!(
            frame(&entries[1]).id,
            Some(CanId {
                raw: 16,
                extended: true
            })
        );
        assert_fd_and_error_frames(&entries);
        let header = trace.blf.as_ref().unwrap().header.as_ref().unwrap();
        assert_eq!(header.size, 144);
        assert_eq!(header.object_count, 5);
        assert_eq!(header.start.year, 2026);
        assert_eq!(header.end.milliseconds, 789);
    }
}

#[test]
fn blf_carries_every_header_and_payload_split_across_containers() {
    let first = object(1, 1, 100, &classic(1, 0, 16));
    let second = object(101, 2, 200, &fd(101, 7 << 12));
    for split in 1..second.len() {
        let mut initial = first.clone();
        initial.extend(&second[..split]);
        let bytes = file(
            &[
                container(&initial, false),
                container(&second[split..], true),
            ],
            2,
        );
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 2, "split {split}");
        assert_eq!(trace.malformed, 0, "split {split}");
        let decoded = entries(&bytes, &trace, &[1]);
        assert_eq!(decoded[0].line, 2);
        assert_eq!(decoded[0].time_ns, Some(2_000_000));
        assert_eq!(frame(&decoded[0]).data.len(), 64);
    }
}

#[test]
fn blf_carries_one_object_through_many_tiny_containers() {
    let inner = object(101, 1, 100, &fd(101, 7 << 12));
    let containers: Vec<_> = inner
        .chunks(3)
        .map(|chunk| container(chunk, true))
        .collect();
    let bytes = file(&containers, 1);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 0);
    assert_eq!(trace.rows(), 1);
    assert_eq!(frame(&entries(&bytes, &trace, &[0])[0]).data.len(), 64);
}

#[test]
fn blf_counts_unknown_object_types_and_accepts_padding() {
    let mut inner = object(97, 1, 0, &[1]);
    inner.extend([0]);
    inner.extend(object(98, 2, 0, &[2, 3]));
    inner.extend([0, 0]);
    inner.extend(object(97, 1, 0, &[]));
    inner.extend(object(1, 1, 100, &classic(1, 0, 16)));
    let outer = object(99, 1, 0, &[]);
    let bytes = file(&[outer, container(&inner, true)], 4);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 0);
    assert_eq!(trace.rows(), 1);
    assert_eq!(trace.lines, 4);
    assert_eq!(
        trace
            .blf
            .as_ref()
            .unwrap()
            .skipped
            .iter()
            .map(|(a, b)| (*a, *b))
            .collect::<Vec<_>>(),
        vec![(97, 2), (98, 1), (99, 1)]
    );
    assert_eq!(entries(&bytes, &trace, &[0])[0].line, 4);
}

#[test]
fn blf_filters_share_the_trace_statistics_and_sparse_window_api() {
    let mut inner = Vec::new();
    for at in 0..200 {
        inner.extend(object(
            1,
            1,
            at,
            &classic(1, 0, if at % 2 == 0 { 16 } else { 32 }),
        ));
    }
    let bytes = file(&[container(&inner, true)], 200);
    let trace = trace(&bytes);
    let key = trace.key_of(0).unwrap();
    assert_eq!(trace.stats[key as usize].count, 100);
    assert_eq!(trace.stats[key as usize].mean_period_ns(), Some(2));
    assert_eq!(
        trace.rows_with_from(key, 190),
        vec![190, 192, 194, 196, 198]
    );
    assert_eq!(trace.previous_with(10), Some(8));
    let decoded = entries(&bytes, &trace, &[199, 64, 0, 64, 200]);
    assert_eq!(
        decoded.iter().map(|entry| entry.row).collect::<Vec<_>>(),
        vec![0, 64, 199]
    );
    assert!(trace.plan(&[200]).is_empty());
}

#[test]
fn logg_sniffing_uses_binary_magic_only_at_offset_zero() {
    assert!(TRACE_EXTENSIONS.contains(&"blf"));
    let bytes = file(&[], 0);
    assert!(sniff_blf(&bytes));
    assert_eq!(sniff_trace(&bytes), Some(Evidence::Header));
    for bytes in [b"LOG".as_slice(), b" LOGG", b"\0LOGG", b"\xEF\xBB\xBFLOGG"] {
        assert!(!sniff_blf(bytes));
        assert_eq!(sniff_trace(bytes), None);
    }
}

#[test]
fn logg_text_and_implausible_headers_are_not_blf() {
    let text = [b"LOGGING started\n".as_slice(), &[b' '; 80]].concat();
    assert!(!sniff_blf(&text));
    assert_eq!(sniff_trace(&text), None);
    let good = file(&[], 0);
    assert!(!sniff_blf(&good[..71]));
    for (at, value) in [
        (4, 71),
        (4, 4097),
        (40, 1979),
        (40, 2101),
        (42, 0),
        (42, 13),
        (46, 0),
        (46, 32),
    ] {
        let mut bytes = good.clone();
        if at == 4 {
            put32(&mut bytes, at, value);
        } else {
            put16(&mut bytes, at, value as u16);
        }
        assert!(!sniff_blf(&bytes), "offset {at} value {value}");
    }
}

#[test]
fn blf_disables_follow_and_rejects_text_append_reads() {
    use rusty_automotive::Watch;
    use std::sync::atomic::AtomicBool;
    let bytes = file(&[container(&object(1, 1, 0, &classic(1, 0, 16)), true)], 1);
    let mut trace = trace(&bytes);
    assert!(!trace.supports_follow());
    let error = trace
        .read_more(
            b"".as_slice(),
            Watch {
                cancel: &AtomicBool::new(false),
                progress: &mut |_| {},
            },
            1,
        )
        .unwrap_err();
    assert!(error.to_string().contains("do not support Follow"));
    assert_eq!(trace.rows(), 1);
    assert!(rusty_automotive::Trace::default().supports_follow());
}

fn assert_fd_and_error_frames(entries: &[rusty_automotive::Entry]) {
    for entry in &entries[2..4] {
        assert_eq!(
            frame(entry).kind,
            FrameKind::Fd {
                brs: true,
                esi: true
            }
        );
        assert_eq!(frame(entry).dlc, 15);
        assert_eq!(frame(entry).data, (0..64).collect::<Vec<u8>>());
        assert_eq!(frame(entry).direction, Some(Direction::Tx));
    }
    assert_eq!(frame(&entries[4]).kind, FrameKind::Error);
    assert_eq!(frame(&entries[4]).data, vec![4, 5, 6]);
    assert_eq!(frame(&entries[4]).direction, Some(Direction::Tx));
    assert_eq!(
        frame(&entries[4]).id,
        Some(CanId {
            raw: 64,
            extended: true
        })
    );
}

#[test]
fn blf_fd_flags_and_valid_lengths_are_independent() {
    for kind in [100, 101] {
        let mut inner = Vec::new();
        for flags in [1, 3, 5, 7] {
            let mut payload = fd(kind, if kind == 100 { flags } else { flags << 12 });
            payload[if kind == 100 { 3 } else { 1 }] = 9;
            payload[if kind == 100 { 14 } else { 2 }] = 12;
            inner.extend(object(kind, 1, 0, &payload));
        }
        let bytes = file(&[container(&inner, true)], 4);
        let trace = trace(&bytes);
        assert_eq!(trace.malformed, 0);
        for (at, entry) in entries(&bytes, &trace, &[0, 1, 2, 3]).iter().enumerate() {
            assert_eq!(
                frame(entry).kind,
                FrameKind::Fd {
                    brs: at % 2 == 1,
                    esi: at >= 2
                }
            );
            assert_eq!(frame(entry).dlc, 9);
            assert_eq!(frame(entry).data, (0..12).collect::<Vec<u8>>());
        }
    }
}

#[test]
fn blf_fd_objects_keep_classic_remote_and_transmit_request_frames() {
    let mut classic_payload = fd(100, 0);
    classic_payload[3] = 8;
    classic_payload[14] = 8;
    let mut remote = fd(101, 16);
    remote[1] = 8;
    remote[2] = 0;
    remote.truncate(40);
    let inner = [
        object(100, 1, 0, &classic_payload),
        object(101, 1, 0, &remote),
        object(101, 1, 0, &fd(101, (1 << 12) | 128)),
    ]
    .concat();
    let bytes = file(&[container(&inner, false)], 3);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 0);
    let decoded = entries(&bytes, &trace, &[0, 1, 2]);
    assert_eq!(frame(&decoded[0]).kind, FrameKind::Can);
    assert_eq!(frame(&decoded[0]).data.len(), 8);
    assert_eq!(frame(&decoded[1]).kind, FrameKind::Remote);
    assert!(frame(&decoded[1]).data.is_empty());
    assert_eq!(frame(&decoded[2]).direction, Some(Direction::TxRequest));
}
