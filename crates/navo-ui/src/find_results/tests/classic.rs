//! Classic form tests: every field round-trips, bad fields report their own field, spans are exact.

use navo_core::find::{FindQuery, TextEncoding};

use crate::find_results::classic::{ClassicField, ClassicForm};

/// A form with a value in every field, each in that token's canonical spelling.
fn full_form() -> ClassicForm {
    ClassicForm {
        text: "needle".into(),
        case: true,
        regex: false,
        word: true,
        not: true,
        hex: String::new(),
        encodings: vec![TextEncoding::Auto, TextEncoding::Utf8],
        names: "*.rs;!test*".into(),
        folders: vec!["/mnt/some dir".into(), "C:\\work files".into()],
        depth: Some(3),
        archives: true,
        git_off: true,
        docs: true,
        ext: "rs,toml".into(),
        size: "<1048576".into(),
        modified: "<7d".into(),
        attrs: "hidden,-readonly".into(),
    }
}

#[test]
fn every_field_survives_text_query_and_back() {
    let form = full_form();
    let text = form.to_text();
    let parsed = FindQuery::parse(&text);
    assert!(
        parsed.tokens.iter().all(|token| token.message.is_none()),
        "clean tokens: {text}"
    );
    assert_eq!(ClassicForm::from_query(&parsed), form, "text: {text}");
    let built = form
        .to_query()
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    assert_eq!(built.to_text(), text);
}

#[test]
fn hex_field_round_trips_on_its_own() {
    let form = ClassicForm {
        hex: "4d5a".into(),
        ..ClassicForm::default()
    };
    assert_eq!(form.to_text(), "hex:4d5a");
    let parsed = FindQuery::parse(&form.to_text());
    assert_eq!(ClassicForm::from_query(&parsed), form);
}

#[test]
fn regex_text_prints_as_one_regex_token() {
    let form = ClassicForm {
        text: "naïve.*".into(),
        regex: true,
        ..ClassicForm::default()
    };
    let text = form.to_text();
    assert_eq!(text, "re:\"naïve.*\"");
    let parsed = FindQuery::parse(&text);
    assert_eq!(parsed.text, "naïve.*");
    assert_eq!(ClassicForm::from_query(&parsed), form);
    let span = form
        .token_span(ClassicField::Regex)
        .expect("a regex marker");
    assert_eq!(&text[span], "re:");
    assert_eq!(form.token_span(ClassicField::Text), Some(0..text.len()));
}

#[test]
fn mid_edit_values_still_print() {
    let form = ClassicForm {
        size: "huge".into(),
        depth: Some(2),
        folders: vec!["/mnt/a b".into()],
        ..ClassicForm::default()
    };
    assert_eq!(form.to_text(), "size:huge in:\"/mnt/a b\" depth:2");
    assert!(form.to_query().is_err());
}

#[test]
fn each_bad_field_reports_its_own_field() {
    let form = ClassicForm {
        hex: "xyz".into(),
        size: "huge".into(),
        depth: Some(999),
        names: ";".into(),
        ..ClassicForm::default()
    };
    let errors = form.to_query().expect_err("four bad fields");
    let fields: Vec<ClassicField> = errors.iter().map(|(field, _)| *field).collect();
    assert_eq!(
        fields,
        vec![
            ClassicField::Size,
            ClassicField::Names,
            ClassicField::Depth,
            ClassicField::Hex
        ]
    );
    for ((field, message), key) in errors.iter().zip(["size:", "name:", "depth:", "hex:"]) {
        assert!(
            message.contains(key),
            "{field:?} should mention {key}: {message}"
        );
    }
}

#[test]
fn token_spans_slice_their_own_token() {
    let form = full_form();
    let text = form.to_text();
    for (field, expected) in [
        (ClassicField::Text, "needle"),
        (ClassicField::Ext, "ext:rs,toml"),
        (ClassicField::Size, "size:<1048576"),
        (ClassicField::Modified, "mod:<7d"),
        (ClassicField::Attrs, "attr:hidden,-readonly"),
        (ClassicField::Archives, "arc:on"),
        (ClassicField::GitOff, "git:off"),
        (ClassicField::Names, "name:*.rs;!test*"),
        (ClassicField::Depth, "depth:3"),
        (ClassicField::Word, "word:on"),
        (ClassicField::Not, "not:on"),
        (ClassicField::Case, "case:on"),
        (ClassicField::Encodings, "enc:auto,utf8"),
        (ClassicField::Docs, "docs:on"),
        (
            ClassicField::Folders,
            "in:\"/mnt/some dir\" in:\"C:\\work files\"",
        ),
    ] {
        let span = form.token_span(field).expect("a printed field");
        assert_eq!(&text[span], expected, "{field:?} in {text}");
    }
}

#[test]
fn spans_land_on_char_boundaries() {
    let form = ClassicForm {
        text: "übergröße".into(),
        names: "*.résumé".into(),
        ..ClassicForm::default()
    };
    let text = form.to_text();
    for (field, expected) in [
        (ClassicField::Text, "übergröße"),
        (ClassicField::Names, "name:*.résumé"),
    ] {
        let span = form.token_span(field).expect("a printed field");
        assert_eq!(&text[span], expected);
    }
}

#[test]
fn absent_tokens_have_no_span() {
    for field in [
        ClassicField::Text,
        ClassicField::Case,
        ClassicField::Regex,
        ClassicField::Word,
        ClassicField::Not,
        ClassicField::Hex,
        ClassicField::Encodings,
        ClassicField::Names,
        ClassicField::Folders,
        ClassicField::Depth,
        ClassicField::Archives,
        ClassicField::GitOff,
        ClassicField::Docs,
        ClassicField::Ext,
        ClassicField::Size,
        ClassicField::Modified,
        ClassicField::Attrs,
    ] {
        assert_eq!(ClassicForm::default().token_span(field), None, "{field:?}");
    }
}
