use std::sync::atomic::AtomicBool;

use super::*;
use crate::window_view::viewer::Kinds;

const DBC: &[u8] = b"BO_ 42 Frame: 8 Sender\n SG_ Speed : 0|8@1+ (1,0) [0|255] \"rpm\" Receiver\n";
const A2L: &[u8] = br#"ASAP2_VERSION 1 71
/begin PROJECT Invented ""
/begin MODULE Demo ""
/begin MEASUREMENT Speed "" UWORD NO_COMPU_METHOD 1 0 0 255
 ECU_ADDRESS 0x1234
/end MEASUREMENT
/end MODULE
/end PROJECT
"#;

#[test]
fn descriptions_pick_rich_and_keep_simple_text_available() {
    for (name, head, id) in [("sample.dbc", DBC, "dbc"), ("sample.a2l", A2L, "a2l")] {
        let sniff = Sniff {
            head,
            name,
            len: head.len() as u64,
        };
        let kinds = Kinds::builtin().choose(&sniff);
        assert_eq!((kinds[0].id(), kinds[0].slot()), (id, Slot::Rich));
        assert!(
            kinds
                .iter()
                .any(|kind| kind.id() == "text" && kind.slot() == Slot::Text)
        );
    }
}

#[test]
fn a_description_extension_without_matching_content_stays_text() {
    for name in ["notes.dbc", "notes.a2l"] {
        let sniff = Sniff {
            head: b"invented notes",
            name,
            len: 14,
        };
        let kinds = Kinds::builtin().choose(&sniff);
        assert_eq!(kinds[0].id(), "text");
        assert!(!kinds.iter().any(|kind| kind.slot() == Slot::Rich));
    }
}

fn load_table(kind: &dyn ViewerKind, bytes: &[u8], cancel: bool) -> Result<TableFile, String> {
    let data = bytes.to_vec();
    let hooks = crate::window_view::viewer::FileHooks {
        range: std::sync::Arc::new(move |_, offset, len| {
            let start = (offset as usize).min(data.len());
            Ok(data[start..(start + len).min(data.len())].to_vec())
        }),
        ..crate::window_view::viewer::FileHooks::none()
    };
    let location = rusty_core::Location::new("/invented/sample").expect("location");
    let sniff = Sniff {
        head: bytes,
        name: "sample",
        len: bytes.len() as u64,
    };
    let cancel = AtomicBool::new(cancel);
    let loaded = kind.load(Load::new(&hooks, &location, &sniff, &cancel))?;
    Ok(*loaded.downcast::<TableFile>().expect("table file"))
}

#[test]
fn dbc_load_flattens_message_and_indented_signal_with_headers() {
    let file = load_table(&DbcKind, DBC, false).expect("DBC loads");
    let TableLoad::Memory { table, .. } = file.load else {
        panic!("in-memory table")
    };
    assert_eq!(file.len, DBC.len() as u64);
    assert_eq!(table.header.len(), 16);
    assert_eq!(table.columns, 16);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(&table.rows[0][..4], ["Frame", "0x2A", "8", "Sender"]);
    assert_eq!(table.rows[1][0], "  Speed");
    assert!(table.rows.iter().all(|row| row.len() == table.header.len()));
}

#[test]
fn a2l_load_flattens_group_and_indented_measurement_with_headers() {
    let file = load_table(&A2lKind, A2L, false).expect("A2L loads");
    let TableLoad::Memory { table, .. } = file.load else {
        panic!("in-memory table")
    };
    assert_eq!(table.header.len(), 9);
    assert_eq!(table.columns, 9);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0][0], "Measurements (1)");
    assert_eq!(&table.rows[1][..3], ["  Speed", "0x1234", "UWORD"]);
    assert!(table.rows.iter().all(|row| row.len() == table.header.len()));
}

#[test]
fn cancelled_description_loads_decline() {
    assert!(load_table(&DbcKind, DBC, true).is_err());
    assert!(load_table(&A2lKind, A2L, true).is_err());
}
