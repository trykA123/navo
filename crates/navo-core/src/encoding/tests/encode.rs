use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::{found, utf16};
use crate::encoding::{DetectedBy, EncodeError, PICKER, TextEncoding, decode_as, encode};

const SAMPLES: &[(&str, &str)] = &[
    ("UTF-8", "naïve ✓ 日本 😀"),
    ("UTF-16LE", "naïve ✓ 日本 😀"),
    ("UTF-16BE", "naïve ✓ 日本 😀"),
    ("windows-1250", "Příliš žluťoučký kůň"),
    ("windows-1251", "Привет мир"),
    ("windows-1252", "café déjà"),
    ("windows-1253", "Καλημέρα"),
    ("windows-1254", "Günaydın şeker"),
    ("windows-1255", "שלום עולם"),
    ("windows-1256", "مرحبا بالعالم"),
    ("windows-1257", "Labas rytas ąčęėįšųū"),
    ("windows-1258", "Xin chào"),
    ("ISO-8859-1", "café déjà"),
    ("ISO-8859-2", "Zażółć gęślą"),
    ("ISO-8859-15", "Prix 10 €"),
    ("KOI8-R", "Привет мир"),
    ("Shift_JIS", "日本語テキスト"),
    ("EUC-JP", "日本語テキスト"),
    ("GBK", "中文文本"),
    ("Big5", "中文文本"),
    ("EUC-KR", "한국어"),
    ("IBM866", "Привет мир"),
];

fn plain(encoding: &'static Encoding) -> TextEncoding {
    found(encoding, false, DetectedBy::Chosen)
}

fn expected_bytes(encoding: &'static Encoding, text: &str) -> Vec<u8> {
    if encoding == UTF_16LE || encoding == UTF_16BE {
        return utf16(text, encoding == UTF_16LE);
    }
    encoding.encode(text).0.into_owned()
}

#[test]
fn every_picker_encoding_round_trips_text_it_can_hold() {
    assert_eq!(PICKER.len(), SAMPLES.len());
    for (label, text) in SAMPLES {
        assert!(PICKER.contains(label), "{label} is offered");
        let encoding = Encoding::for_label(label.as_bytes()).unwrap();
        let bytes = encode(text, &plain(encoding)).unwrap();
        assert_eq!(bytes, expected_bytes(encoding, text), "{label}");
        let decoded = decode_as(&bytes, encoding);
        assert_eq!(decoded.text, *text, "{label}");
        assert!(decoded.lossless, "{label}");
    }
}

#[test]
fn the_bom_is_written_back() {
    let utf8 = encode("hi", &found(UTF_8, true, DetectedBy::Bom)).unwrap();
    assert_eq!(utf8, [0xEF, 0xBB, 0xBF, b'h', b'i']);
    let little = encode("hi", &found(UTF_16LE, true, DetectedBy::Bom)).unwrap();
    assert_eq!(little, [0xFF, 0xFE, b'h', 0, b'i', 0]);
    let big = encode("hi", &found(UTF_16BE, true, DetectedBy::Bom)).unwrap();
    assert_eq!(big, [0xFE, 0xFF, 0, b'h', 0, b'i']);
}

#[test]
fn utf16_little_endian_puts_the_low_byte_first() {
    let bytes = encode("hé😀", &plain(UTF_16LE)).unwrap();
    assert_eq!(bytes, [0x68, 0, 0xE9, 0, 0x3D, 0xD8, 0x00, 0xDE]);
}

#[test]
fn utf16_big_endian_puts_the_high_byte_first() {
    let bytes = encode("hé😀", &plain(UTF_16BE)).unwrap();
    assert_eq!(bytes, [0, 0x68, 0, 0xE9, 0xD8, 0x3D, 0xDE, 0x00]);
}

#[test]
fn an_unmappable_character_names_its_line() {
    let refused = encode("first\nsecond\nthird 日 here\n", &plain(WINDOWS_1252));
    assert_eq!(
        refused,
        Err(EncodeError::Unmappable {
            line: 3,
            character: '日'
        })
    );
}

#[test]
fn labels_read_like_an_editor_status_bar() {
    let cases = [
        (found(UTF_8, false, DetectedBy::Utf8), "UTF-8"),
        (found(UTF_8, true, DetectedBy::Bom), "UTF-8 BOM"),
        (found(UTF_16LE, true, DetectedBy::Bom), "UTF-16 LE BOM"),
        (found(UTF_16BE, false, DetectedBy::Chosen), "UTF-16 BE"),
        (
            found(WINDOWS_1252, false, DetectedBy::Guess),
            "Windows-1252",
        ),
    ];
    for (encoding, label) in cases {
        assert_eq!(encoding.label(), label);
    }
}
