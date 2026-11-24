//! Tests for line transforms and case conversion, including permutation and idempotence props.

use proptest::prelude::*;

use super::*;

/// Returns default sort options for `kind`.
fn spec(kind: SortKind) -> SortSpec {
    SortSpec {
        kind,
        descending: false,
        ignore_case: false,
    }
}

#[test]
fn sort_alpha_keeps_crlf_and_final_ending() {
    let out = sort_lines("b\r\na\r\n", spec(SortKind::Alpha));
    assert_eq!(out, "a\r\nb\r\n");
}

#[test]
fn sort_natural_orders_numbers_by_value() {
    let out = sort_lines("file10\nfile2\nFile1", spec(SortKind::Natural));
    assert_eq!(out, "File1\nfile2\nfile10");
}

#[test]
fn sort_alpha_ignore_case_is_stable() {
    let out = sort_lines(
        "b\nA\na",
        SortSpec {
            kind: SortKind::Alpha,
            descending: false,
            ignore_case: true,
        },
    );
    assert_eq!(out, "A\na\nb");
}

#[test]
fn sort_length_descending() {
    let out = sort_lines(
        "aa\nb\nccc",
        SortSpec {
            kind: SortKind::Length,
            descending: true,
            ignore_case: false,
        },
    );
    assert_eq!(out, "ccc\naa\nb");
}

#[test]
fn sort_reverse_flips_order() {
    let out = sort_lines("1\n2\n3\n", spec(SortKind::Reverse));
    assert_eq!(out, "3\n2\n1\n");
}

#[test]
fn dedup_keeps_first_anywhere() {
    assert_eq!(dedup_lines("a\nb\na\n\nb\n\n"), "a\nb\n\n");
}

#[test]
fn join_drops_endings() {
    assert_eq!(join_lines("a\nb\n", ", "), "a, b");
}

#[test]
fn trim_trailing_keeps_endings() {
    assert_eq!(trim_trailing("a  \r\nb\t\r\n"), "a\r\nb\r\n");
}

#[test]
fn case_snake_camel_pascal_kebab() {
    let text = "  HTTPServer fooBar v2Beta utf8  ";
    assert_eq!(
        convert_case(text, TextCase::Snake),
        "  http_server_foo_bar_v2_beta_utf8  "
    );
    assert_eq!(
        convert_case(text, TextCase::Camel),
        "  httpServerFooBarV2BetaUtf8  "
    );
    assert_eq!(
        convert_case(text, TextCase::Pascal),
        "  HttpServerFooBarV2BetaUtf8  "
    );
    assert_eq!(
        convert_case(text, TextCase::Kebab),
        "  http-server-foo-bar-v2-beta-utf8  "
    );
}

#[test]
fn case_title_uses_name_case() {
    assert_eq!(convert_case("hello world", TextCase::Title), "Hello World");
    assert_eq!(
        convert_case("hello world", TextCase::Sentence),
        "Hello world"
    );
}

proptest! {
    #[test]
    fn sort_is_a_permutation(lines in prop::collection::vec("[a-zA-Z0-9 ]{0,8}", 0..20)) {
        let text = lines.join("\n");
        let parsed = split_lines(&text);
        let mut expected: Vec<&str> = parsed.lines.clone();
        expected.sort_unstable();
        let final_ending = text.ends_with('\n');
        for kind in [SortKind::Alpha, SortKind::Natural, SortKind::Length, SortKind::Reverse] {
            let output = sort_lines(&text, spec(kind));
            if parsed.lines.is_empty() {
                prop_assert!(output.is_empty());
                continue;
            }
            let body = if final_ending {
                output.strip_suffix('\n').unwrap_or(&output)
            } else {
                output.as_str()
            };
            let mut actual: Vec<&str> = body.split('\n').collect();
            actual.sort_unstable();
            prop_assert_eq!(&actual, &expected, "kind {:?}, text {:?}", kind, text);
        }
    }

    #[test]
    fn dedup_is_idempotent(lines in prop::collection::vec("[a-zA-Z0-9 ]{0,8}", 0..20)) {
        let text = lines.join("\n");
        let once = dedup_lines(&text);
        let twice = dedup_lines(&once);
        prop_assert_eq!(twice, once);
    }
}
