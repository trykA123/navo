use std::fmt::Write as _;

mod support;

use proptest::prelude::*;
use rusty_automotive::Body;
use support::{every_entry, trace_of};

fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("Rx".to_owned()),
        Just("Tx".to_owned()),
        Just("d".to_owned()),
        Just("r".to_owned()),
        Just("CANFD".to_owned()),
        Just("ErrorFrame".to_owned()),
        Just("base".to_owned()),
        Just("hex".to_owned()),
        Just("dec".to_owned()),
        Just("timestamps".to_owned()),
        Just("relative".to_owned()),
        "[0-9]{1,3}\\.[0-9]{0,9}",
        "[0-9A-Fa-f]{1,9}x?",
        "[ -~]{0,6}",
    ]
}

fn lines() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::collection::vec(token(), 0..14), 0..40).prop_map(|lines| {
        lines
            .into_iter()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn frame_line() -> impl Strategy<Value = (u32, bool, Vec<u8>)> {
    (
        0u32..0x7FF,
        any::<bool>(),
        prop::collection::vec(any::<u8>(), 0..=8),
    )
}

proptest! {
    #[test]
    fn random_bytes_never_panic_and_every_row_decodes(bytes in prop::collection::vec(any::<u8>(), 0..2000)) {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let trace = trace_of(&text);
        prop_assert_eq!(every_entry(&text, &trace).len() as u64, trace.rows());
    }

    #[test]
    fn random_token_lines_never_panic_and_every_row_decodes(text in lines()) {
        let trace = trace_of(&text);
        let entries = every_entry(&text, &trace);
        prop_assert_eq!(entries.len() as u64, trace.rows());
        let malformed = entries.iter().filter(|entry| matches!(entry.body, Body::Malformed { .. })).count();
        prop_assert_eq!(malformed as u64, trace.malformed);
    }

    #[test]
    fn written_frames_read_back(frames in prop::collection::vec(frame_line(), 1..80)) {
        let mut text = String::from("base hex  timestamps absolute\n");
        for (at, (id, extended, data)) in frames.iter().enumerate() {
            let bytes: Vec<String> = data.iter().map(|byte| format!("{byte:02X}")).collect();
            let suffix = if *extended { "x" } else { "" };
            writeln!(text,
                "{}.{:06} 1 {id:X}{suffix} Rx d {} {}",
                at / 1000,
                at % 1000,
                data.len(),
                bytes.join(" ")
            ).unwrap();
        }
        let trace = trace_of(&text);
        let entries = every_entry(&text, &trace);
        prop_assert_eq!(entries.len(), frames.len());
        for (entry, (id, extended, data)) in entries.iter().zip(&frames) {
            let Body::Frame(frame) = &entry.body else {
                return Err(TestCaseError::fail(format!("{entry:?}")));
            };
            prop_assert_eq!(frame.id.map(|found| (found.raw, found.extended)), Some((*id, *extended)));
            prop_assert_eq!(&frame.data, data);
        }
    }

    #[test]
    fn reading_in_chunks_matches_reading_at_once(frames in prop::collection::vec(frame_line(), 1..120), cut in 1u64..50) {
        let mut text = String::new();
        for (at, (id, _, data)) in frames.iter().enumerate() {
            let bytes: Vec<String> = data.iter().map(|byte| format!("{byte:02X}")).collect();
            writeln!(text, "{at}.5 1 {id:X} Rx d {} {}", data.len(), bytes.join(" ")).unwrap();
        }
        let whole = trace_of(&text);
        let chunked = support::chunked_trace(&text, cut);
        prop_assert_eq!(chunked.rows(), whole.rows());
        prop_assert_eq!(&chunked.stats, &whole.stats);
        prop_assert_eq!(every_entry(&text, &chunked), every_entry(&text, &whole));
    }
}
