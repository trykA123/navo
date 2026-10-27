use rusty_automotive::{
    CanId, Endian,
    dbc::{Message, Multiplexing, Number, Signal, SignalFormat, bit_layout, formula},
};

fn signal(name: &str, start_bit: u64, length: u64) -> Signal {
    Signal {
        line: 1,
        name: name.into(),
        start_bit,
        length,
        byte_order: Endian::Little,
        signed: false,
        format: SignalFormat::Integer,
        factor: 1.0,
        offset: 0.0,
        min: Number::Unsigned(0),
        max: Number::Unsigned(0),
        unit: String::new(),
        receivers: Vec::new(),
        multiplexing: Multiplexing::Plain,
    }
}

fn message(dlc: u64, signals: Vec<Signal>) -> Message {
    Message {
        line: 1,
        id: CanId {
            raw: 42,
            extended: false,
        },
        name: "Frame".into(),
        dlc,
        sender: None,
        cycle_time_ms: None,
        signals,
    }
}

#[test]
fn intel_sixteen_bits_fill_the_first_two_bytes() {
    let layout = bit_layout(&message(64, vec![signal("Reading", 0, 16)]));
    assert_eq!(layout.bytes, 64);
    assert_eq!(layout.cells.len(), 512);
    assert!(layout.problems.is_empty());
    for byte in 0..64 {
        for bit in 0..8 {
            let expected: &[u16] = if byte < 2 { &[0] } else { &[] };
            assert_eq!(layout.cell(byte, bit).signals, expected);
        }
    }
}

#[test]
fn motorola_twelve_bits_follow_the_sawtooth_from_bit_seven() {
    let mut reading = signal("Reading", 7, 12);
    reading.byte_order = Endian::Big;
    let layout = bit_layout(&message(2, vec![reading]));
    assert!(layout.problems.is_empty());
    for byte in 0..2 {
        for bit in 0..8 {
            let expected: &[u16] = if byte == 0 || bit >= 4 { &[0] } else { &[] };
            assert_eq!(layout.cell(byte, bit).signals, expected);
        }
    }
}

#[test]
fn overlapping_signals_report_the_names_and_linear_bit() {
    let layout = bit_layout(&message(
        2,
        vec![signal("First", 7, 2), signal("Second", 8, 1)],
    ));
    assert_eq!(layout.cell(1, 0).signals, [0, 1]);
    assert_eq!(layout.problems, ["First and Second overlap at bit 8"]);
}

#[test]
fn different_multiplex_values_share_bits_without_problems() {
    let mut first = signal("First", 0, 1);
    first.multiplexing = Multiplexing::Value(1);
    let mut second = signal("Second", 0, 1);
    second.multiplexing = Multiplexing::Value(2);
    let layout = bit_layout(&message(2, vec![first, second]));
    assert_eq!(layout.cell(0, 0).signals, [0, 1]);
    assert!(layout.problems.is_empty());
}

#[test]
fn signal_past_two_byte_dlc_reports_the_byte() {
    let layout = bit_layout(&message(2, vec![signal("Reading", 15, 2)]));
    assert_eq!(layout.cell(1, 7).signals, [0]);
    assert_eq!(layout.problems, ["Reading runs past byte 2"]);
}

#[test]
fn signed_formula_uses_shortest_factor_and_offset() {
    let mut reading = signal("Reading", 0, 16);
    reading.signed = true;
    reading.factor = 0.5;
    reading.offset = -10.0;
    assert_eq!(formula(&reading), "phys = raw × 0.5 + -10 (signed 16 bit)");
}

#[test]
fn unsigned_formula_includes_the_bit_length() {
    assert_eq!(
        formula(&signal("Reading", 0, 3)),
        "phys = raw × 1 + 0 (unsigned 3 bit)"
    );
}

#[test]
fn floating_formula_uses_float_for_both_float_formats() {
    let mut reading = signal("Reading", 0, 32);
    reading.format = SignalFormat::Float;
    reading.signed = true;
    assert_eq!(formula(&reading), "phys = raw × 1 + 0 (float 32 bit)");
    reading.format = SignalFormat::Double;
    reading.length = 64;
    assert_eq!(formula(&reading), "phys = raw × 1 + 0 (float 64 bit)");
}
