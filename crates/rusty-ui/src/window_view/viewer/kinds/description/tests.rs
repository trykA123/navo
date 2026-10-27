use std::sync::atomic::AtomicBool;

use rusty_automotive::dbc;

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

fn load_description(
    kind: &dyn ViewerKind,
    bytes: &[u8],
    cancel: bool,
) -> Result<DescriptionFile, String> {
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
    Ok(*loaded
        .downcast::<DescriptionFile>()
        .expect("description file"))
}

#[test]
fn dbc_load_keeps_the_model_and_builds_its_outline() {
    let file = load_description(&DbcKind, DBC, false).expect("DBC loads");
    assert_eq!(file.len, DBC.len() as u64);
    let DescModel::Dbc(model) = file.model.as_ref() else {
        panic!("DBC model")
    };
    assert_eq!(model.messages[0].name, "Frame");
    assert_eq!(file.outline.as_ref(), &dbc::outline(model));
    assert_eq!(file.outline.rows[2].label.as_ref(), "Speed");
}

#[test]
fn a2l_load_keeps_the_model_links_and_module_outline() {
    let file = load_description(&A2lKind, A2L, false).expect("A2L loads");
    assert_eq!(file.len, A2L.len() as u64);
    let DescModel::A2l(model, links) = file.model.as_ref() else {
        panic!("A2L model")
    };
    assert_eq!(model.measurements[0].name, "Speed");
    assert!(links.missing.is_empty());
    assert_eq!(links.users.len(), model.compu_methods.len());
    assert_eq!(file.outline.as_ref(), &a2l::outline(model));
    assert!(file.outline.rows[0].label.starts_with("Module Demo"));
}

#[test]
fn cancelled_description_loads_decline() {
    assert!(load_description(&DbcKind, DBC, true).is_err());
    assert!(load_description(&A2lKind, A2L, true).is_err());
}
