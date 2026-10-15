use super::{EDIT_LIMIT, Kinds, Shape, Slot, Sniff, ViewerKind};
use crate::window_view::viewer::PagedKind;

fn sniff(head: &[u8], len: u64) -> Sniff<'_> {
    Sniff {
        head,
        name: "file.bin",
        len,
    }
}

fn first(head: &[u8], len: u64) -> (&'static str, Shape) {
    let sniff = sniff(head, len);
    let chosen = Kinds::builtin().choose(&sniff);
    let best = chosen.first().expect("a kind accepts every file");
    (best.id(), best.shape(&sniff))
}

#[test]
fn utf8_text_opens_in_the_text_editor() {
    let text = "naïve café\n".as_bytes();
    assert_eq!(first(text, text.len() as u64), ("text", Shape::Editor));
}

#[test]
fn a_nul_byte_or_a_utf16_bom_opens_as_hex() {
    let hex = ("hex", Shape::Paged(PagedKind::Hex));
    assert_eq!(first(b"MZ\x90\x00\x03", 5), hex);
    assert_eq!(first(&[0xFF, 0xFE, b'a', b'b'], 4), hex);
}

#[test]
fn text_over_16_mb_opens_as_paged_text() {
    let paged = ("text", Shape::Paged(PagedKind::Text));
    assert_eq!(first(b"line\n", EDIT_LIMIT + 1), paged);
    assert_eq!(first(b"line\n", EDIT_LIMIT).1, Shape::Editor);
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

    fn shape(&self, _: &Sniff<'_>) -> Shape {
        Shape::Paged(PagedKind::Hex)
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
