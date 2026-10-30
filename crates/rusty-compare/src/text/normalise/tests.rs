use rusty_core::paging::TextLayout;

use super::*;
use crate::model::CaseRule;
use crate::text::side::{Line, LineEnding, Whole};

fn key(text: &str, ending: LineEnding, options: &Options) -> String {
    normalise(text, ending, false, options)
}

fn bom_key(text: &str, ending: LineEnding, options: &Options) -> String {
    normalise(text, ending, true, options)
}

fn line(text: &str, ending: LineEnding) -> Line {
    Line {
        text: text.to_owned(),
        ending,
        bom: false,
    }
}

#[test]
fn whitespace_all_ignores_every_space() {
    let options = Options {
        whitespace: Whitespace::All,
        ..Options::default()
    };

    assert_eq!(
        key(" a b\tc ", LineEnding::None, &options),
        key("abc", LineEnding::None, &options)
    );
}

#[test]
fn whitespace_changes_ignores_the_amount_but_not_the_presence() {
    let options = Options {
        whitespace: Whitespace::Changes,
        ..Options::default()
    };

    assert_eq!(
        key("  a  b ", LineEnding::None, &options),
        key(" a b", LineEnding::None, &options)
    );
    assert_ne!(
        key(" a", LineEnding::None, &options),
        key("a", LineEnding::None, &options)
    );
    assert_eq!(
        key("a  ", LineEnding::None, &options),
        key("a", LineEnding::None, &options)
    );
}

#[test]
fn whitespace_trailing_ignores_only_the_end() {
    let options = Options {
        whitespace: Whitespace::Trailing,
        ..Options::default()
    };

    assert_eq!(
        key("a  ", LineEnding::None, &options),
        key("a", LineEnding::None, &options)
    );
    assert_ne!(
        key(" a", LineEnding::None, &options),
        key("a", LineEnding::None, &options)
    );
    assert_ne!(
        key("a  b", LineEnding::None, &options),
        key("a b", LineEnding::None, &options)
    );
}

#[test]
fn line_endings_can_be_ignored() {
    let kept = Options::default();
    assert_ne!(
        key("a", LineEnding::Lf, &kept),
        key("a", LineEnding::CrLf, &kept)
    );

    let ignored = Options {
        ignore_line_endings: true,
        ..Options::default()
    };
    assert_eq!(
        key("a", LineEnding::Lf, &ignored),
        key("a", LineEnding::CrLf, &ignored)
    );
}

#[test]
fn case_can_be_ignored() {
    let sensitive = Options::default();
    assert_ne!(
        key("Hello", LineEnding::None, &sensitive),
        key("hello", LineEnding::None, &sensitive)
    );

    let insensitive = Options {
        case: CaseRule::Insensitive,
        ..Options::default()
    };
    assert_eq!(
        key("Hello", LineEnding::None, &insensitive),
        key("hello", LineEnding::None, &insensitive)
    );
}

#[test]
fn insensitive_case_folds_sharp_s_and_sigma() {
    let options = Options {
        case: CaseRule::Insensitive,
        ..Options::default()
    };

    assert_eq!(
        key("Straße", LineEnding::None, &options),
        key("STRASSE", LineEnding::None, &options)
    );
    assert_eq!(
        key("σ", LineEnding::None, &options),
        key("ς", LineEnding::None, &options)
    );
    assert_eq!(
        key("σ", LineEnding::None, &options),
        key("Σ", LineEnding::None, &options)
    );
}

#[test]
fn a_bom_can_be_ignored() {
    let kept = Options::default();
    assert_ne!(
        bom_key("a", LineEnding::None, &kept),
        key("a", LineEnding::None, &kept)
    );

    let ignored = Options {
        ignore_bom: true,
        ..Options::default()
    };
    assert_eq!(
        bom_key("a", LineEnding::None, &ignored),
        key("a", LineEnding::None, &ignored)
    );
}

#[test]
fn keys_are_produced_per_line() {
    let whole = Whole {
        layout: TextLayout::default(),
        lines: vec![line("a", LineEnding::Lf), line("b", LineEnding::None)],
        starts: vec![0, 2, 3],
        lossless: true,
    };
    let keys = whole.keys(&Options::default());

    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1]);
}
