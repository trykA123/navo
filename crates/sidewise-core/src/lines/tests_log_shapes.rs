//! Log shape checks: URL, path and GUID spans and their boundaries.
use super::log_shapes::shape;
use super::*;

/// Lexes one complete line from a clean start and returns its spans.
fn lex_spans(line: &str) -> Vec<Span> {
    LineLanguage::Log.lex_last(line, LineState(0)).0
}

#[test]
fn colours_drive_path_and_drops_a_trailing_comma() {
    assert_eq!(
        lex_spans("C:\\Work\\Prüfung\\ecu.arxml,"),
        vec![(0..26, "link_uri")]
    );
}

#[test]
fn colours_unc_path_and_stops_at_a_space() {
    assert_eq!(lex_spans("\\\\srv\\share x"), vec![(0..11, "link_uri")]);
}

#[test]
fn colours_posix_path() {
    assert_eq!(lex_spans("/usr/lib/x.so"), vec![(0..13, "link_uri")]);
}

#[test]
fn a_single_slash_word_is_not_a_path() {
    assert!(lex_spans("/x").is_empty());
}

#[test]
fn colours_url_and_drops_a_trailing_dot() {
    assert_eq!(lex_spans("see https://a.b/c."), vec![(4..17, "link_uri")]);
}

#[test]
fn colours_braced_guid_including_the_braces() {
    assert_eq!(
        lex_spans("{6F9619FF-8B86-D011-B42D-00C04FC964FF}"),
        vec![(0..38, "constructor")]
    );
}

#[test]
fn shape_returns_the_length_and_role() {
    assert_eq!(shape(b"https://a.b/c.", 0), Some((13, "link_uri")));
    assert_eq!(shape(b"C:\\x", 0), Some((4, "link_uri")));
    assert_eq!(shape(b"/x/y", 0), Some((4, "link_uri")));
    assert_eq!(shape(b"/x", 0), None);
    assert_eq!(
        shape(b"6F9619FF-8B86-D011-B42D-00C04FC964FF", 0),
        Some((36, "constructor"))
    );
    assert_eq!(
        shape(b"{6F9619FF-8B86-D011-B42D-00C04FC964FF}", 0),
        Some((38, "constructor"))
    );
}

#[test]
fn shape_rejects_a_word_byte_before_or_after() {
    let guid = "6F9619FF-8B86-D011-B42D-00C04FC964FF";
    assert_eq!(shape(format!("x{guid}").as_bytes(), 1), None);
    assert_eq!(shape(format!("{guid}x").as_bytes(), 0), None);
    assert_eq!(shape(format!("x{{{guid}}}").as_bytes(), 1), None);
    assert_eq!(shape(b"aC:/x", 1), None);
}

#[test]
fn url_scheme_is_case_insensitive_and_stops_at_markers() {
    assert_eq!(shape(b"HTTP://a.b/c,", 0), Some((12, "link_uri")));
    assert_eq!(shape(b"http://a/b)", 0), Some((10, "link_uri")));
    assert_eq!(shape(b"ftp://x", 0), Some((7, "link_uri")));
    assert_eq!(shape(b"file://x", 0), Some((8, "link_uri")));
    assert_eq!(shape(b"httpx://a", 0), None);
}

#[test]
fn path_ends_at_markers_and_drops_trailing_punctuation() {
    assert_eq!(shape(b"\\\\srv\\share x", 0), Some((11, "link_uri")));
    assert_eq!(shape(b"/x/", 0), Some((3, "link_uri")));
    assert_eq!(shape(b"C:\\a\\b),", 0), Some((6, "link_uri")));
    assert_eq!(shape(b"C:\\a\" b", 0), Some((4, "link_uri")));
    assert_eq!(shape(b"C:\\a|b", 0), Some((4, "link_uri")));
    assert_eq!(shape(b"C:\\a?b", 0), Some((4, "link_uri")));
    assert_eq!(shape("C:\\Prüfung".as_bytes(), 0), Some((11, "link_uri")));
}

#[test]
fn guid_needs_group_lengths_hex_and_braces() {
    assert_eq!(shape(b"6F9619FF-8B86-D011-B42D-00C04FC964F", 0), None);
    assert_eq!(shape(b"6F9619FG-8B86-D011-B42D-00C04FC964FF", 0), None);
    assert_eq!(shape(b"{6F9619FF-8B86-D011-B42D-00C04FC964FF", 0), None);
    assert_eq!(
        shape(b"6F9619FF-8B86-D011-B42D-00C04FC964FF", 0),
        Some((36, "constructor"))
    );
}

#[test]
fn colours_date_and_t_separated_time_as_one_span() {
    assert_eq!(
        lex_spans("2026-11-18T09:15:02.123Z INFO"),
        vec![(0..24, "attribute"), (25..29, "function")]
    );
}

#[test]
fn colours_date_and_space_separated_time_as_two_spans() {
    assert_eq!(
        lex_spans("18.11.2026 09:15"),
        vec![(0..10, "attribute"), (11..16, "attribute")]
    );
}

#[test]
fn a_slash_date_is_not_a_path() {
    assert_eq!(lex_spans("2026/11/18"), vec![(0..10, "attribute")]);
}

#[test]
fn a_short_minute_is_not_a_time() {
    assert_eq!(lex_spans("12:5"), vec![(0..2, "number"), (3..4, "number")]);
}

#[test]
fn an_out_of_range_month_is_not_a_date() {
    assert_eq!(
        lex_spans("2026-13-01"),
        vec![(0..4, "number"), (5..7, "number"), (8..10, "number")]
    );
}

#[test]
fn colours_every_date_format() {
    for line in [
        "2026-11-18",
        "2026/11/18",
        "2026.11.18",
        "18.11.2026",
        "18/11/2026",
    ] {
        assert_eq!(lex_spans(line), vec![(0..10, "attribute")], "{line}");
    }
}

#[test]
fn time_forms_end_at_a_boundary() {
    assert_eq!(lex_spans("9:15"), vec![(0..4, "attribute")]);
    assert_eq!(lex_spans("09:15:02"), vec![(0..8, "attribute")]);
    assert_eq!(lex_spans("09:15:02,5"), vec![(0..10, "attribute")]);
    assert_eq!(lex_spans("09:15Z"), vec![(0..6, "attribute")]);
    assert_eq!(lex_spans("09:15+05:30"), vec![(0..11, "attribute")]);
    assert_eq!(lex_spans("09:15-0500"), vec![(0..10, "attribute")]);
    for line in ["09:15abc", "24:00", "12:60"] {
        assert!(
            lex_spans(line).iter().all(|(_, role)| *role != "attribute"),
            "{line}"
        );
    }
}

#[test]
fn shape_colours_dates_and_times_as_attribute() {
    assert_eq!(shape(b"2026-11-18", 0), Some((10, "attribute")));
    assert_eq!(shape(b"18.11.2026", 0), Some((10, "attribute")));
    assert_eq!(
        shape(b"2026-11-18T09:15:02.123Z", 0),
        Some((24, "attribute"))
    );
    assert_eq!(shape(b"2026-11-18T09:15", 0), Some((16, "attribute")));
    assert_eq!(shape(b"09:15", 0), Some((5, "attribute")));
    assert_eq!(shape(b"9:15:02+05:30", 0), Some((13, "attribute")));
    assert_eq!(shape(b"2026/11/18", 0), Some((10, "attribute")));
    assert_eq!(shape(b"2026-13-01", 0), None);
    assert_eq!(shape(b"2026-11-18abc", 0), None);
    assert_eq!(shape(b"12:5", 0), None);
    assert_eq!(shape(b"09:15abc", 0), None);
    assert_eq!(shape(b"24:00", 0), None);
}
