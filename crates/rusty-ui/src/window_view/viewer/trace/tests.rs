use rusty_automotive::{Base, Body, CanId, Direction, Entry, Frame, FrameKind, TraceFormat};

use super::text::{Tone, binary, column_titles, format_label, id, row, span, time};

fn entry(body: Body) -> Entry {
    Entry {
        row: 0,
        line: 1,
        time_ns: Some(1_234_567_000),
        body,
    }
}

#[test]
fn times_show_microseconds_and_spans_pick_their_unit() {
    assert_eq!(time(1_234_567_891), "1.234567");
    assert_eq!(span(10_000_000), "10.000 ms");
    assert_eq!(span(2_500_000_000), "2.500 s");
}

#[test]
fn ids_are_hex_with_an_x_for_extended() {
    assert_eq!(
        id(
            CanId {
                raw: 0x7DF,
                extended: false
            },
            Base::Hex
        ),
        "7DF"
    );
    assert_eq!(
        id(
            CanId {
                raw: 0x18FE_F100,
                extended: true
            },
            Base::Hex
        ),
        "18FEF100x"
    );
    let dec = CanId {
        raw: 291,
        extended: true,
    };
    assert_eq!(id(dec, Base::Dec), "291x");
}

#[test]
fn a_byte_shows_in_binary_as_two_nibbles() {
    assert_eq!(binary(0xA5), "1010 0101");
}

#[test]
fn a_frame_row_lines_up_under_the_column_titles() {
    let frame = Frame {
        channel: 1,
        id: Some(CanId {
            raw: 0x123,
            extended: false,
        }),
        direction: Some(Direction::Rx),
        kind: FrameKind::Can,
        dlc: 2,
        data: vec![0x01, 0xFF],
    };
    let (text, tone) = row(&entry(Body::Frame(frame)), (Base::Hex, TraceFormat::Asc));
    assert_eq!(tone, Tone::Plain);
    assert_eq!(
        text,
        "    1.234567  1   123         Rx    CAN       2  01 FF"
    );
    assert_eq!(text.find("123"), column_titles().find("ID"));
    assert_eq!(text.find("01 FF"), column_titles().find("Data"));
}

#[test]
fn error_frames_and_other_lines_have_their_own_tone() {
    let error = Frame {
        channel: 2,
        id: None,
        direction: None,
        kind: FrameKind::Error,
        dlc: 0,
        data: Vec::new(),
    };
    assert_eq!(
        row(&entry(Body::Frame(error)), (Base::Hex, TraceFormat::Asc)).1,
        Tone::Error
    );
    let (text, tone) = row(
        &entry(Body::Other("1.234567 Start of measurement".to_owned())),
        (Base::Hex, TraceFormat::Asc),
    );
    assert_eq!(tone, Tone::Quiet);
    assert!(
        text.ends_with("Other        Start of measurement"),
        "{text}"
    );
}

#[test]
fn the_header_names_the_file_format() {
    assert_eq!(format_label(TraceFormat::Asc), "Vector ASC");
    assert_eq!(format_label(TraceFormat::Trc(Some("2.1"))), "PEAK TRC 2.1");
    assert_eq!(format_label(TraceFormat::Trc(None)), "PEAK TRC");
}

#[test]
fn trc_event_rows_drop_the_message_number_and_the_time() {
    let trc = (Base::Hex, TraceFormat::Trc(Some("1.1")));
    for text in ["1) 4.500 Warng FFFFFFFF 4", "1 4.500 Warng FFFFFFFF 4"] {
        let (line, _) = row(&entry(Body::Other(text.to_owned())), trc);
        assert!(line.ends_with("Other        Warng FFFFFFFF 4"), "{line}");
        assert_eq!(line.matches("1.234567").count(), 1, "{line}");
    }
}
