mod blf_build;

use std::io::Cursor;
use std::sync::atomic::AtomicBool;

use blf_build::*;
use rusty_automotive::{Direction, IndexError, Watch, index_blf};

#[test]
fn corrupt_inner_objects_keep_the_valid_prefix_and_stop() {
    let good = object(1, 1, 1, &classic(1, 0, 16));
    let mut bad_headers = Vec::new();
    for (at, value) in [(4, 0), (4, 5000), (8, 1), (8, u32::MAX), (16, 0), (16, 3)] {
        let mut bad = good.clone();
        put32(&mut bad, at, value);
        bad_headers.push(bad);
    }
    let mut bad = good.clone();
    bad[0] = b'X';
    bad_headers.push(bad);
    let mut bad = good.clone();
    put16(&mut bad, 6, 3);
    bad_headers.push(bad);
    let mut bad = good.clone();
    put32(&mut bad, 16, 1);
    bad[24..32].copy_from_slice(&u64::MAX.to_le_bytes());
    bad_headers.push(bad);
    for bad in bad_headers {
        let inner = [good.clone(), bad, good.clone()].concat();
        let bytes = file(&[container(&inner, true)], 3);
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 1);
        assert_eq!(trace.malformed, 1);
        assert_eq!(trace.problems[0].line, 2);
        assert_eq!(
            trace.blf.as_ref().unwrap().problem_position.unwrap().inner,
            good.len() as u32
        );
        assert_eq!(entries(&bytes, &trace, &[0]).len(), 1);
    }
}

#[test]
fn truncated_inner_objects_keep_frames_before_the_split() {
    let good = object(1, 1, 1, &classic(1, 0, 16));
    for length in 1..good.len() {
        let inner = [good.clone(), good[..length].to_vec()].concat();
        let bytes = file(&[container(&inner, true)], 2);
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 1, "length {length}");
        assert_eq!(trace.malformed, 1, "length {length}");
        assert_eq!(entries(&bytes, &trace, &[0]).len(), 1);
    }
}

#[test]
fn truncated_file_and_container_headers_never_panic() {
    let inner = object(1, 1, 1, &classic(1, 0, 16));
    let whole = container(&inner, true);
    let padded = whole.len() - u32::from_le_bytes(whole[8..12].try_into().unwrap()) as usize;
    let bytes = file(&[whole], 1);
    for length in 0..bytes.len() {
        let trace = trace(&bytes[..length]);
        if length >= bytes.len() - padded {
            assert_eq!((trace.malformed, trace.rows()), (0, 1), "length {length}");
        } else if length == 144 {
            assert_eq!((trace.malformed, trace.rows()), (0, 0));
        } else {
            assert_eq!((trace.malformed, trace.rows()), (1, 0), "length {length}");
        }
        assert!(!trace.supports_follow());
    }
}

#[test]
fn corrupt_containers_keep_prior_containers_and_report_the_problem() {
    let inner = object(1, 1, 1, &classic(1, 0, 16));
    let good = container(&inner, true);
    let mut bad_containers = Vec::new();
    for (at, value) in [(4, u32::MAX), (8, u32::MAX), (24, u32::MAX), (24, 1)] {
        let mut bad = good.clone();
        put32(&mut bad, at, value);
        bad_containers.push(bad);
    }
    let mut bad = good.clone();
    put16(&mut bad, 16, 1);
    bad_containers.push(bad);
    let mut bad = good.clone();
    bad[32] = 0;
    bad_containers.push(bad);
    let mut bad = good.clone();
    let packed_end = u32::from_le_bytes(bad[8..12].try_into().unwrap()) as usize;
    bad[packed_end - 1] ^= 255;
    bad_containers.push(bad);
    let mut bad = good.clone();
    put32(&mut bad, 8, (packed_end - 1) as u32);
    bad.truncate(packed_end - 1);
    bad.extend(std::iter::repeat_n(0, bad.len() % 4));
    bad_containers.push(bad);
    for bad in bad_containers {
        let bytes = file(&[good.clone(), bad, good.clone()], 3);
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 1);
        assert_eq!(trace.malformed, 1);
        assert_eq!(entries(&bytes, &trace, &[0]).len(), 1);
    }
}

#[test]
fn oversized_file_headers_are_rejected_without_large_reads() {
    for size in [0, 71, 4097, u32::MAX] {
        let mut bytes = file(&[], 0);
        put32(&mut bytes, 4, size);
        let mut largest = 0;
        let source = rusty_automotive::Source::new(
            |at, count| {
                largest = largest.max(count);
                let start = (at as usize).min(bytes.len());
                Ok(bytes[start..(start + count).min(bytes.len())].to_vec())
            },
            bytes.len() as u64,
        );
        let trace = index_blf(
            source,
            Watch {
                cancel: &AtomicBool::new(false),
                progress: &mut |_| {},
            },
        )
        .unwrap();
        assert_eq!(trace.malformed, 1);
        assert!(largest <= 72);
    }
}

#[test]
fn oversized_inflation_fields_and_compression_bombs_are_bounded() {
    let inner = object(1, 1, 1, &classic(1, 0, 16));
    for compressed in [false, true] {
        let mut bad = container(&inner, compressed);
        put32(&mut bad, 24, 64 * 1024 * 1024 + 1);
        let bytes = file(&[bad], 1);
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 0);
        assert!(trace.problems[0].reason.contains("64 MiB"));
    }
    let mut bomb = container(&vec![0; 1_000_000], true);
    put32(&mut bomb, 24, 1);
    let bytes = file(&[bomb], 1);
    let trace = trace(&bytes);
    assert_eq!(trace.rows(), 0);
    assert!(trace.problems[0].reason.contains("inflated size"));
}

fn invalid_objects() -> Vec<Vec<u8>> {
    let mut invalid = Vec::new();
    let mut payload = classic(1, 0, 16);
    payload[3] = 16;
    invalid.push(object(1, 1, 0, &payload));
    invalid.push(object(1, 1, 0, &classic(1, 0, 1 << 29)));
    invalid.push(object(86, 1, 0, &classic(1, 0, 16)));
    for kind in [100, 101] {
        let mut payload = fd(kind, if kind == 100 { 7 } else { 7 << 12 });
        payload[if kind == 100 { 14 } else { 2 }] = 65;
        invalid.push(object(kind, 1, 0, &payload));
    }
    let mut payload = fd(101, 7 << 12);
    payload[34] = 3;
    invalid.push(object(101, 1, 0, &payload));
    invalid.push(object(101, 1, 0, &[0; 39]));
    invalid.push(object(73, 1, 0, &[0; 23]));
    invalid
}

#[test]
fn invalid_can_payloads_are_counted_and_later_frames_are_kept() {
    let good = object(1, 1, 1, &classic(1, 0, 16));
    for bad in invalid_objects() {
        let inner = [good.clone(), bad.clone(), good.clone()].concat();
        let bytes = file(&[container(&inner, false)], 3);
        let trace = trace(&bytes);
        assert_eq!(trace.rows(), 2);
        assert_eq!(trace.malformed, 1);
        assert_eq!(trace.problems.len(), 1);
        assert_eq!(trace.problems[0].line, 2);
        assert_eq!(trace.blf.as_ref().unwrap().problem_position, None);
        assert_eq!(entries(&bytes, &trace, &[0, 1]).len(), 2);
    }
}

#[test]
fn window_ordinals_agree_with_the_index_across_a_bad_frame() {
    let mut inner = Vec::new();
    for at in 0..100u64 {
        let bad = at % 10 == 3;
        let payload = if bad {
            classic(1, 0, 1 << 29)
        } else {
            classic(1, 0, 16)
        };
        inner.extend(object(1, 1, at, &payload));
    }
    let bytes = file(&[container(&inner, true)], 100);
    let trace = trace(&bytes);
    assert_eq!(trace.rows(), 90);
    assert_eq!(trace.malformed, 10);
    assert_eq!(trace.lines, 100);
    let wanted = [0, 2, 3, 40, 41, 89];
    let decoded = entries(&bytes, &trace, &wanted);
    let lines: Vec<_> = decoded.iter().map(|entry| entry.line).collect();
    assert_eq!(lines, vec![1, 3, 5, 46, 47, 100]);
    for entry in &decoded {
        assert_eq!(entry.time_ns, Some(entry.line - 1));
    }
}

#[test]
fn rejected_frame_problems_are_capped() {
    let inner: Vec<u8> = (0..1100)
        .flat_map(|_| object(1, 1, 0, &classic(1, 0, 1 << 29)))
        .collect();
    let bytes = file(&[container(&inner, true)], 1100);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 1100);
    assert_eq!(trace.problems.len(), 1000);
    assert_eq!(trace.rows(), 0);
}

#[test]
fn classic_dlc_above_eight_is_legal_and_keeps_the_logged_value() {
    let mut inner = Vec::new();
    for dlc in [9, 12, 15] {
        let mut payload = classic(1, 0, 16);
        payload[3] = dlc;
        inner.extend(object(1, 1, 0, &payload));
    }
    let bytes = file(&[container(&inner, false)], 3);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 0);
    for (entry, dlc) in entries(&bytes, &trace, &[0, 1, 2]).iter().zip([9, 12, 15]) {
        assert_eq!(frame(entry).dlc, dlc);
        assert_eq!(frame(entry).data, (0..8).collect::<Vec<u8>>());
    }
}

#[test]
fn fd64_direction_codes_map_and_unknown_codes_are_malformed() {
    let mut inner = Vec::new();
    for code in [0, 1, 2, 3] {
        let mut payload = fd(101, 7 << 12);
        payload[34] = code;
        payload[35] = 200;
        inner.extend(object(101, 1, 0, &payload));
    }
    let bytes = file(&[container(&inner, false)], 4);
    let trace = trace(&bytes);
    assert_eq!(trace.rows(), 3);
    assert_eq!(trace.malformed, 1);
    let directions: Vec<_> = entries(&bytes, &trace, &[0, 1, 2])
        .iter()
        .map(|entry| frame(entry).direction)
        .collect();
    assert_eq!(
        directions,
        vec![
            Some(Direction::Rx),
            Some(Direction::Tx),
            Some(Direction::TxRequest)
        ]
    );
}

#[test]
fn missing_padding_after_the_last_container_keeps_its_frames() {
    let mut payload = classic(1, 0, 16);
    payload.extend([0, 0]);
    let inner = [
        object(1, 1, 0, &payload),
        object(1, 1, 1, &classic(1, 0, 16)),
    ]
    .concat();
    let whole = container(&inner, false);
    let padding = whole.len() - 32 - inner.len();
    assert!(padding > 0);
    let mut bytes = file(&[whole], 2);
    bytes.truncate(bytes.len() - padding);
    let trace = trace(&bytes);
    assert_eq!(trace.malformed, 0);
    assert_eq!(trace.rows(), 2);
    assert_eq!(entries(&bytes, &trace, &[0, 1]).len(), 2);
}

#[test]
fn blf_index_reports_progress_and_observes_cancellation() {
    let cancel = AtomicBool::new(false);
    let inner = object(1, 1, 0, &classic(1, 0, 16));
    let bytes = file(&[container(&inner, true), container(&inner, false)], 2);
    let mut progress = Vec::new();
    let trace = index_blf(
        Cursor::new(&bytes),
        Watch {
            cancel: &cancel,
            progress: &mut |at| progress.push(at),
        },
    )
    .unwrap();
    assert_eq!(trace.rows(), 2);
    assert_eq!(progress.first(), Some(&0));
    assert_eq!(progress.last(), Some(&(bytes.len() as u64)));
    assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
    let error = index_blf(
        Cursor::new(&bytes),
        Watch {
            cancel: &cancel,
            progress: &mut |_| cancel.store(true, std::sync::atomic::Ordering::Relaxed),
        },
    )
    .unwrap_err();
    assert_eq!(error, IndexError::Cancelled);
}

proptest::proptest! {
    #[test]
    fn arbitrary_input_and_compressed_payloads_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..1024)) {
        let _ = trace(&bytes);
        let file = file(&[container(&bytes, true)], 1);
        let _ = trace(&file);
    }
}

#[test]
fn blf_read_failures_are_reported_by_indexing_and_window_decode() {
    let bytes = file(&[container(&object(1, 1, 0, &classic(1, 0, 16)), true)], 1);
    let trace = trace(&bytes);
    let error = trace
        .plan(&[0])
        .decode(&mut |_, _| Err("invented read failure".into()))
        .unwrap_err();
    assert!(error.contains("invented read failure"));
    let source = rusty_automotive::Source::new(
        |_, _| Err("invented read failure".into()),
        bytes.len() as u64,
    );
    let error = index_blf(
        source,
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap_err();
    assert!(matches!(error, IndexError::Read(_)));
}
