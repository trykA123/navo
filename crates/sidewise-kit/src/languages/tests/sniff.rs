//! Content-sniff tests for D1a: markup roots and UTF-16 heads.
//! Each case runs through `detect`, the same entry point as the language tests.

use super::super::detect;

/// Encodes `text` as a BOM-prefixed UTF-16 head in the requested byte order.
/// Returns the raw bytes; only the head is needed by the sniff.
fn utf16(text: &str, big_endian: bool) -> Vec<u8> {
    let mut bytes = if big_endian {
        vec![0xfe, 0xff]
    } else {
        vec![0xff, 0xfe]
    };
    for unit in text.encode_utf16() {
        bytes.extend(if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        });
    }
    bytes
}

#[test]
fn markup_roots_give_xml() {
    for head in [
        "<AUTOSAR>",
        "<_root>",
        "<!-- c -->",
        "<!DOCTYPE html>",
        "<!doctype svg>",
    ] {
        assert_eq!(detect("notes.dat", head.as_bytes()), "xml", "{head:?}");
    }
}

#[test]
fn leading_space_and_bom_are_skipped() {
    for head in [
        "  \n<AUTOSAR>",
        "\u{feff}<AUTOSAR>",
        "\u{feff} \t<!-- c -->",
    ] {
        assert_eq!(detect("notes.dat", head.as_bytes()), "xml", "{head:?}");
    }
}

#[test]
fn non_markup_angles_stay_text() {
    for head in [
        "<134>Oct 11 12:00:00 host sshd[1]: started",
        "<1>",
        "< root>",
    ] {
        assert_eq!(detect("notes.dat", head.as_bytes()), "text", "{head:?}");
    }
    assert_eq!(detect("app.log", b"started"), "text");
}

#[test]
fn utf16_heads_decode_before_sniffing() {
    let prolog = "<?xml version=\"1.0\"?>\n<a/>";
    assert_eq!(detect("notes.dat", &utf16(prolog, false)), "xml");
    assert_eq!(detect("notes.dat", &utf16(prolog, true)), "xml");
    assert_eq!(detect("notes.dat", &utf16("<!-- c -->", false)), "xml");
}

#[test]
fn utf16_odd_trailing_byte_is_dropped() {
    let mut head = utf16("<AUTOSAR>", false);
    head.push(b'x');
    assert_eq!(detect("notes.dat", &head), "xml");
}

#[test]
fn json_heads_give_json() {
    for head in [
        "{ \"a\": 1 }",
        "{}",
        "  {\n  \"a\"",
        "[1, 2]",
        "[true]",
        "[\n  false,\n  null\n]",
        "[[1]]",
        "[{\"a\": 1}]",
        "[\"x\"]",
        "[-1]",
        "[]",
    ] {
        assert_eq!(detect("config.dat", head.as_bytes()), "json", "{head:?}");
    }
}

#[test]
fn non_json_braces_and_brackets_stay_text() {
    for head in [
        "[network]",
        "[trueish]",
        "[falsehood]",
        "[nullify]",
        "{abc",
        "{",
        "[",
        "   ",
    ] {
        assert_eq!(detect("notes.dat", head.as_bytes()), "text", "{head:?}");
    }
}

#[test]
fn diagnostic_extensions_give_xml() {
    for name in [
        "ecu.cdd", "a.odx", "a.odx-d", "a.odx-c", "a.odx-v", "a.odx-f", "a.odx-e", "A.ODX",
    ] {
        assert_eq!(detect(name, b""), "xml", "{name}");
    }
    assert_eq!(detect("a.pdx", b""), "text");
}

#[test]
fn known_extensions_ignore_markup_content() {
    assert_eq!(detect("main.rs", b"<AUTOSAR>"), "rust");
    assert_eq!(detect("main.rs", &utf16("<!-- c -->", false)), "rust");
}
