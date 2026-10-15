use rusty_core::Location;

use super::{EDIT_LIMIT, Kinds, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::FileHooks;
use crate::window_view::viewer::body::{Attach, ViewerBody};

fn sniff(head: &[u8], len: u64) -> Sniff<'_> {
    Sniff {
        head,
        name: "file.bin",
        len,
    }
}

fn first(head: &[u8], len: u64) -> (&'static str, bool) {
    let sniff = sniff(head, len);
    let chosen = Kinds::builtin().choose(&sniff);
    let best = chosen.first().expect("a kind accepts every file");
    (best.id(), best.editable(&sniff))
}

#[test]
fn utf8_text_opens_in_the_text_editor() {
    let text = "naïve café\n".as_bytes();
    assert_eq!(first(text, text.len() as u64), ("text", true));
}

fn utf16(text: &str, little: bool, bom: bool) -> Vec<u8> {
    let bom = if bom { Some('\u{FEFF}') } else { None };
    bom.into_iter()
        .chain(text.chars())
        .collect::<String>()
        .encode_utf16()
        .flat_map(|unit| {
            if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            }
        })
        .collect()
}

#[test]
fn utf16_with_or_without_a_bom_opens_as_text_and_binary_as_hex() {
    assert_eq!(first(b"MZ\x90\x00\x03", 5), ("hex", false));
    for (little, bom) in [(true, true), (false, true), (true, false), (false, false)] {
        let bytes = utf16("Some notes, written on Windows.\r\n", little, bom);
        assert_eq!(
            first(&bytes, bytes.len() as u64),
            ("text", true),
            "little {little}, bom {bom}"
        );
    }
}

#[test]
fn utf16_over_the_edit_limit_opens_as_hex() {
    for (little, bom) in [(true, true), (false, true), (true, false)] {
        let bytes = utf16("Some notes, written on Windows.\r\n", little, bom);
        assert_eq!(
            first(&bytes, EDIT_LIMIT + 1),
            ("hex", false),
            "little {little}, bom {bom}"
        );
        assert_eq!(first(&bytes, EDIT_LIMIT), ("text", true));
    }
}

#[test]
fn text_over_16_mb_opens_as_paged_text() {
    assert_eq!(first(b"line\n", EDIT_LIMIT + 1), ("text", false));
    assert_eq!(first(b"line\n", EDIT_LIMIT), ("text", true));
}

#[test]
fn text_and_hex_accept_every_file_best_first() {
    let sniff = sniff(b"plain", 5);
    let ids: Vec<_> = Kinds::builtin()
        .choose(&sniff)
        .iter()
        .map(|kind| (kind.id(), kind.slot()))
        .collect();
    assert_eq!(ids, [("text", Slot::Text), ("hex", Slot::Hex)]);
}

struct Pictures;

impl ViewerKind for Pictures {
    fn id(&self) -> &'static str {
        "pictures"
    }

    fn name(&self) -> &'static str {
        "Picture"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.head.starts_with(b"\x89PNG") {
            90
        } else {
            0
        }
    }

    fn load(&self, _: &FileHooks, _: &Location, _: &Sniff<'_>) -> Result<Loaded, String> {
        Ok(Box::new(()))
    }

    fn attach(
        &self,
        _: Attach,
        _: &mut Window,
        _: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        Err("No picture view in this test.".to_owned())
    }
}

#[test]
fn an_added_kind_with_a_higher_score_wins_and_others_still_accept() {
    let kinds = Kinds::builtin().with(Pictures);
    let png = sniff(b"\x89PNG\r\n\x1a\n\x00", 9);
    let ids: Vec<_> = kinds.choose(&png).iter().map(|kind| kind.id()).collect();
    assert_eq!(ids, ["pictures", "hex", "text"]);
    let text = sniff(b"hello", 5);
    let ids: Vec<_> = kinds.choose(&text).iter().map(|kind| kind.id()).collect();
    assert_eq!(ids, ["text", "hex"]);
}

fn best_named(name: &str, head: &[u8], len: u64) -> &'static str {
    let sniff = Sniff { head, name, len };
    Kinds::builtin()
        .choose(&sniff)
        .first()
        .map(|kind| kind.id())
        .expect("a kind accepts every file")
}

#[test]
fn structured_files_open_as_tree_table_or_markdown() {
    let cases: [(&str, &[u8], &str); 14] = [
        ("shop.json", b" {\"a\": 1}", "tree"),
        ("list.json", b"\xEF\xBB\xBF[1, 2]", "tree"),
        ("settings.jsonc", b"// note\n{}", "tree"),
        ("app.json5", b"{a: 1}", "tree"),
        ("ci.yaml", b"name: build\n", "tree"),
        ("ci.yml", b"- one\n", "tree"),
        ("model.arxml", b"<?xml version=\"1.0\"?><AUTOSAR/>", "tree"),
        ("app.csproj", b"<Project Sdk=\"x\"/>", "tree"),
        ("info.plist", b"<?xml version=\"1.0\"?><plist/>", "tree"),
        ("icon.svg", b"<svg xmlns=\"x\"/>", "tree"),
        ("fruit.csv", b"name,qty\nPear,1\n", "table"),
        ("fruit.tsv", b"name\tqty\nPear\t1\n", "table"),
        ("notes.md", b"# Title\n", "markdown"),
        ("notes.markdown", b"Text\n", "markdown"),
    ];
    for (name, head, kind) in cases {
        assert_eq!(best_named(name, head, head.len() as u64), kind, "{name}");
    }
}

#[test]
fn plain_text_and_content_that_does_not_fit_stay_text() {
    assert_eq!(best_named("notes.txt", b"{\"a\": 1}", 8), "text");
    assert_eq!(best_named("shop.json", b"hello", 5), "text");
    assert_eq!(best_named("page.xml", b"plain words", 11), "text");
    assert_eq!(
        best_named("fruit.csv", b"\x00\x01\x02\x03binary", 10),
        "hex"
    );
    let huge = rusty_structured::TREE_LIMIT + 1;
    assert_eq!(best_named("shop.json", b"{\"a\": 1}", huge), "text");
    assert_eq!(best_named("ci.yaml", b"a: 1\n", huge), "text");
    assert_eq!(best_named("model.arxml", b"<AUTOSAR>", huge), "tree");
    assert_eq!(best_named("notes.md", b"# Title\n", EDIT_LIMIT + 1), "text");
    let sniff = Sniff {
        head: b"{\"a\": 1}",
        name: "shop.json",
        len: huge,
    };
    let note = Kinds::builtin()
        .declined(&sniff)
        .expect("the tree says why");
    assert!(note.contains("64 MB"), "{note}");
}

struct Vector;

impl ViewerKind for Vector {
    fn id(&self) -> &'static str {
        "image"
    }

    fn name(&self) -> &'static str {
        "Image"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.name.ends_with(".svg") { 90 } else { 0 }
    }

    fn load(&self, _: &FileHooks, _: &Location, _: &Sniff<'_>) -> Result<Loaded, String> {
        Ok(Box::new(()))
    }

    fn attach(
        &self,
        _: Attach,
        _: &mut Window,
        _: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        Err("No image view in this test.".to_owned())
    }
}

#[test]
fn svg_stays_an_image_when_the_image_kind_exists() {
    let kinds = Kinds::builtin().with(Vector);
    let sniff = Sniff {
        head: b"<svg xmlns=\"x\"/>",
        name: "icon.svg",
        len: 17,
    };
    let ids: Vec<_> = kinds.choose(&sniff).iter().map(|kind| kind.id()).collect();
    assert_eq!(ids.first(), Some(&"image"));
    assert!(ids.contains(&"tree"));
}
