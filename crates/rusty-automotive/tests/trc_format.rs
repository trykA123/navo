mod support;

use rusty_automotive::{CanId, Evidence, TraceFormat, sniff_trace};
use support::{every_entry, frame, trace_of};

#[test]
fn a_trace_names_its_format() {
    let trc = trace_of(";$FILEVERSION=2.1\n;$COLUMNS=N,O,T,B,I,d,R,L,D\n");
    assert_eq!(trc.format(), TraceFormat::Trc(Some("2.1")));
    let asc = trace_of("date Mon Jan 1 12:00:00 am 2024\nbase hex timestamps absolute\n");
    assert_eq!(asc.format(), TraceFormat::Asc);
}

#[test]
fn an_unsupported_trc_version_does_not_sniff() {
    for version in ["1.2", "3.0", ""] {
        let head = format!(";$FILEVERSION={version}\n;$STARTTIME=45000\n");
        assert_eq!(sniff_trace(head.as_bytes()), None, "{version}");
    }
    assert_eq!(sniff_trace(b";$FILEVERSION=2.0\n"), Some(Evidence::Header));
}

#[test]
fn trc_ids_take_one_to_eight_hex_digits() {
    let text = ";$FILEVERSION=1.1\n1) 1.0 Rx 7DF 1 01\n2) 2.0 Rx 12 1 01\n3) 3.0 Rx 0800 1 01\n4) 4.0 Rx 18FEF1 1 01\n5) 5.0 Rx 1FFFFFFF 1 01\n6) 6.0 Rx 20000000 1 01\n";
    let trace = trace_of(text);
    let entries = every_entry(text, &trace);
    let id = |at: usize| frame(&entries[at]).id;
    assert_eq!(
        id(0),
        Some(CanId {
            raw: 0x7DF,
            extended: false
        })
    );
    assert_eq!(
        id(1),
        Some(CanId {
            raw: 0x12,
            extended: false
        })
    );
    assert_eq!(
        id(2),
        Some(CanId {
            raw: 0x800,
            extended: true
        })
    );
    assert_eq!(
        id(3),
        Some(CanId {
            raw: 0x18FEF1,
            extended: true
        })
    );
    assert_eq!(
        id(4),
        Some(CanId {
            raw: 0x1FFF_FFFF,
            extended: true
        })
    );
    assert_eq!(trace.malformed, 1);
}
