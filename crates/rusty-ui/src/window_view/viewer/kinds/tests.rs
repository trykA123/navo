use super::{EDIT_LIMIT, Kinds, Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
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
            95
        } else {
            0
        }
    }

    fn load(&self, _: Load<'_>) -> Result<Loaded, String> {
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
    assert_eq!(ids, ["pictures", "image", "hex", "text"]);
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
    let cases: [(&str, &[u8], &str); 13] = [
        ("shop.json", b" {\"a\": 1}", "tree"),
        ("list.json", b"\xEF\xBB\xBF[1, 2]", "tree"),
        ("settings.jsonc", b"// note\n{}", "tree"),
        ("app.json5", b"{a: 1}", "tree"),
        ("ci.yaml", b"name: build\n", "tree"),
        ("ci.yml", b"- one\n", "tree"),
        ("model.arxml", b"<?xml version=\"1.0\"?><AUTOSAR/>", "tree"),
        ("app.csproj", b"<Project Sdk=\"x\"/>", "tree"),
        ("info.plist", b"<?xml version=\"1.0\"?><plist/>", "tree"),
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
    assert_eq!(
        best_named("notes.md", b"# Title\n", 1024 * 1024),
        "markdown"
    );
    assert_eq!(
        best_named("notes.md", b"# Title\n", 1024 * 1024 + 1),
        "text"
    );
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

#[test]
fn svg_stays_an_image_when_the_image_kind_exists() {
    let kinds = Kinds::builtin();
    let sniff = Sniff {
        head: b"<svg xmlns=\"x\"/>",
        name: "icon.svg",
        len: 17,
    };
    let ids: Vec<_> = kinds.choose(&sniff).iter().map(|kind| kind.id()).collect();
    assert_eq!(ids.first(), Some(&"image"));
    assert!(ids.contains(&"tree"));
}

const ARCHIVO: &[u8] = include_bytes!("../../../../fonts/Archivo-Regular.ttf");

fn best(head: &[u8], name: &str) -> &'static str {
    let sniff = Sniff {
        head,
        name,
        len: head.len() as u64,
    };
    let chosen = Kinds::builtin().choose(&sniff);
    chosen.first().expect("a kind accepts every file").id()
}

fn ustar() -> Vec<u8> {
    let mut block = vec![0; 1024];
    block[..9].copy_from_slice(b"notes.txt");
    block[257..263].copy_from_slice(b"ustar\0");
    block
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).expect("gzip");
    encoder.finish().expect("gzip")
}

#[test]
fn archive_magic_and_tar_names_pick_the_archive_view() {
    let cases: [(&[u8], &str); 7] = [
        (b"PK\x03\x04\x14\x00\x00\x00", "report.docx"),
        (b"PK\x05\x06\x00\x00\x00\x00", "empty.zip"),
        (b"7z\xBC\xAF\x27\x1C\x00\x04", "backup.7z"),
        (&ustar(), "logs.tar"),
        (&gzip(&ustar()), "logs.gz"),
        (b"\xFD7zXZ\x00\x00\x04", "logs.tar.xz"),
        (b"\x28\xB5\x2F\xFD\x00\x58", "logs.tzst"),
    ];
    for (head, name) in cases {
        assert_eq!(best(head, name), "archive", "{name}");
    }
}

#[test]
fn font_magic_picks_the_font_view() {
    let otf = b"OTTO\x00\x0A\x00\x80\x00\x03\x00\x20CFF \x00\x00\x00\x00";
    let cases: [(&[u8], &str); 4] = [
        (ARCHIVO, "Archivo-Regular.ttf"),
        (otf, "Display.otf"),
        (b"ttcf\x00\x01\x00\x00\x00\x00\x00\x02", "Family.ttc"),
        (b"wOFF\x00\x01\x00\x00\x00\x00\x10\x00", "Web.woff"),
    ];
    for (head, name) in cases {
        assert_eq!(best(head, name), "font", "{name}");
    }
}

#[test]
fn other_files_stay_with_text_or_hex() {
    assert_eq!(best(b"plain words\n", "notes.txt"), "text");
    assert_eq!(best(b"plain words\n", "notes.tar"), "text");
    assert_eq!(best(&gzip(b"just a log"), "app.log.gz"), "hex");
    assert_eq!(best(b"wOF2\x00\x01\x00\x00", "Web.woff2"), "hex");
    assert_eq!(best(b"\x00\x01\x00\x00\xFF\xFF", "data.bin"), "hex");
}

#[test]
fn every_image_format_opens_as_an_image_by_content_or_name() {
    let heads: [&[u8]; 9] = [
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR",
        b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00",
        b"GIF89a\x01\x00\x01\x00",
        b"GIF87a\x01\x00\x01\x00",
        b"RIFF\x24\x00\x00\x00WEBPVP8L",
        b"BM\x46\x00\x00\x00\x00\x00",
        b"\x00\x00\x01\x00\x01\x00\x10\x10",
        b"II*\x00\x08\x00\x00\x00",
        b"MM\x00*\x00\x00\x00\x08",
    ];
    for head in heads {
        assert_eq!(best(head, "file.bin"), "image", "{head:?}");
    }
    let svg = b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    assert_eq!(best(svg, "logo.svg"), "image");
    assert_eq!(best(svg, "logo.SVGZ"), "image");
    assert_eq!(best(b"\x00\x01\x02\x03", "scan.tif"), "image");
}

#[test]
fn text_and_other_binaries_do_not_open_as_images() {
    assert_eq!(best(b"BMW 320d, blue\n", "cars.txt"), "text");
    assert_eq!(best(b"plain words\n", "named.png"), "text");
    assert_eq!(best(b"<p>an <svg> tag in prose</p>\n", "page.html"), "text");
    assert_eq!(best(b"MZ\x90\x00\x03", "setup.exe"), "hex");
    assert_eq!(best(b"\x00\x00\x01\x00\x00\x00", "data.bin"), "hex");
}

#[test]
fn an_svg_inside_a_text_file_can_still_be_shown_as_an_image() {
    let head = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n";
    let sniff = Sniff {
        head,
        name: "drawing.xml",
        len: head.len() as u64,
    };
    let ids: Vec<_> = Kinds::builtin()
        .choose(&sniff)
        .iter()
        .map(|kind| (kind.id(), kind.slot()))
        .collect();
    assert_eq!(
        ids,
        [
            ("tree", Slot::Rich),
            ("text", Slot::Text),
            ("image", Slot::Rich),
            ("hex", Slot::Hex)
        ]
    );
}

#[test]
fn pdf_bytes_open_the_pdf_view_and_text_and_hex_stay_available() {
    let head = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj";
    let sniff = sniff(head, 4096);
    let slots: Vec<(&str, Slot)> = Kinds::builtin()
        .choose(&sniff)
        .iter()
        .map(|kind| (kind.id(), kind.slot()))
        .collect();
    assert_eq!(slots.first(), Some(&("pdf", Slot::Rich)));
    assert!(slots.contains(&("text", Slot::Text)));
    assert!(slots.contains(&("hex", Slot::Hex)));
}

#[test]
fn a_pdf_name_without_the_pdf_header_is_not_a_pdf() {
    let named = Sniff {
        head: b"just some notes",
        name: "notes.pdf",
        len: 15,
    };
    let ids: Vec<&str> = Kinds::builtin()
        .choose(&named)
        .iter()
        .map(|kind| kind.id())
        .collect();
    assert!(!ids.contains(&"pdf"), "{ids:?}");
}
