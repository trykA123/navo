use crate::rename::{CounterSpec, LocalTime, Mask, MaskError, MaskErrorKind, Span, Subject, Token};

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 3,
    day: 7,
    hour: 9,
    minute: 5,
    second: 1,
};

fn subject<'a>(stem: &'a str, ext: &'a str) -> Subject<'a> {
    Subject {
        stem,
        ext,
        parent: "Photos",
        grand: "Trips",
        time: Some(TIME),
        fields: None,
    }
}

fn mask(text: &str) -> Mask {
    Mask::parse(text).unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
}

fn render(text: &str, stem: &str, ext: &str) -> String {
    mask(text).render(&subject(stem, ext), 0)
}

fn error(text: &str) -> MaskError {
    Mask::parse(text).expect_err("mask should not parse")
}

#[test]
fn plain_text_is_kept() {
    assert_eq!(render("holiday", "a", "b"), "holiday");
}

#[test]
fn empty_mask_renders_nothing() {
    assert_eq!(render("", "a", "b"), "");
}

#[test]
fn name_and_extension_whole() {
    assert_eq!(render("[N].[E]", "report", "pdf"), "report.pdf");
}

#[test]
fn at_takes_one_character() {
    assert_eq!(render("[N3]", "abcdef", ""), "c");
}

#[test]
fn range_is_inclusive() {
    assert_eq!(render("[N2-4]", "abcdef", ""), "bcd");
}

#[test]
fn len_counts_from_the_start_position() {
    assert_eq!(render("[N2,3]", "abcdef", ""), "bcd");
}

#[test]
fn from_runs_to_the_end() {
    assert_eq!(render("[N3-]", "abcdef", ""), "cdef");
}

#[test]
fn extension_uses_the_same_spans() {
    assert_eq!(render("[E2-]", "x", "json"), "son");
    assert_eq!(render("[E-2,2]", "x", "json"), "on");
}

#[test]
fn negative_at_counts_from_the_end() {
    assert_eq!(render("[N-1]", "abcdef", ""), "f");
    assert_eq!(render("[N-6]", "abcdef", ""), "a");
}

#[test]
fn negative_start_with_length() {
    assert_eq!(render("[N-3,3]", "abcdef", ""), "def");
}

#[test]
fn negative_end_in_a_range() {
    assert_eq!(render("[N2--2]", "abcdef", ""), "bcde");
}

#[test]
fn negative_start_and_end_in_a_range() {
    assert_eq!(render("[N-4--2]", "abcdef", ""), "cde");
}

#[test]
fn negative_start_runs_to_the_end() {
    assert_eq!(render("[N-2-]", "abcdef", ""), "ef");
}

#[test]
fn positions_outside_the_name_give_empty_text() {
    assert_eq!(render("[N9]", "abc", ""), "");
    assert_eq!(render("[N-9]", "abc", ""), "");
    assert_eq!(render("[N9-]", "abc", ""), "");
    assert_eq!(render("[N9,2]", "abc", ""), "");
    assert_eq!(render("[N9-12]", "abc", ""), "");
    assert_eq!(render("[N0]", "abc", ""), "");
}

#[test]
fn an_end_past_the_name_stops_at_the_name() {
    assert_eq!(render("[N2-9]", "abc", ""), "bc");
    assert_eq!(render("[N2,9]", "abc", ""), "bc");
}

#[test]
fn a_range_that_runs_backwards_is_empty() {
    assert_eq!(render("[N4-2]", "abcdef", ""), "");
    assert_eq!(render("[N2--9]", "abcdef", ""), "");
}

#[test]
fn zero_length_is_empty() {
    assert_eq!(render("[N2,0]", "abc", ""), "");
}

#[test]
fn an_empty_part_gives_empty_text() {
    assert_eq!(render("[N]-[E]", "", ""), "-");
    assert_eq!(render("[N1]", "", ""), "");
}

#[test]
fn positions_count_characters_not_bytes() {
    assert_eq!(render("[N2]", "日本語", ""), "本");
    assert_eq!(render("[N2-3]", "ñandú☃", ""), "an");
    assert_eq!(render("[N-1]", "ñandú☃", ""), "☃");
    assert_eq!(render("[N-2,2]", "ñandú☃", ""), "ú☃");
}

#[test]
fn parent_and_grandparent() {
    assert_eq!(render("[G]-[P]-[N]", "a", ""), "Trips-Photos-a");
}

#[test]
fn counter_defaults_to_one_and_counts_up() {
    let m = mask("[C]");
    let s = subject("a", "");
    assert_eq!(m.render(&s, 0), "1");
    assert_eq!(m.render(&s, 1), "2");
    assert_eq!(m.render(&s, 11), "12");
}

#[test]
fn counter_with_its_own_start_step_and_digits() {
    let m = mask("[C10+5:3]");
    let s = subject("a", "");
    assert_eq!(m.render(&s, 0), "010");
    assert_eq!(m.render(&s, 1), "015");
    assert_eq!(m.render(&s, 2), "020");
}

#[test]
fn counter_wider_than_its_digits_is_not_cut() {
    assert_eq!(mask("[C998+1:2]").render(&subject("a", ""), 1), "999");
}

#[test]
fn counter_uses_the_caller_default_when_bare() {
    let default = CounterSpec {
        start: 7,
        step: 2,
        digits: 4,
    };
    let s = subject("a", "");
    assert_eq!(mask("[C]").render_with(&s, 3, default), "0013");
    assert_eq!(mask("[C1+1:1]").render_with(&s, 3, default), "4");
}

#[test]
fn counter_does_not_overflow() {
    let big = format!("[C{}+9:1]", u64::MAX);
    assert_eq!(
        mask(&big).render(&subject("a", ""), 5),
        u64::MAX.to_string()
    );
}

#[test]
fn date_tokens_use_four_and_two_digits() {
    assert_eq!(
        render("[Y]-[M]-[D] [h][i][s]", "a", ""),
        "2026-03-07 090501"
    );
}

#[test]
fn date_tokens_are_empty_without_a_time() {
    let s = Subject {
        time: None,
        ..subject("a", "")
    };
    assert_eq!(mask("x[Y][y][M][D][h][i][s][d][t]y").render(&s, 0), "xy");
}

#[test]
fn year_is_padded_to_four_digits() {
    let s = Subject {
        time: Some(LocalTime { year: 987, ..TIME }),
        ..subject("a", "")
    };
    assert_eq!(mask("[Y]").render(&s, 0), "0987");
}

#[test]
fn short_year_date_and_time() {
    assert_eq!(render("[y]", "a", ""), "26");
    assert_eq!(render("[d]", "a", ""), "2026-03-07");
    assert_eq!(render("[t]", "a", ""), "09.05.01");
}

#[test]
fn short_year_keeps_leading_zero() {
    let s = Subject {
        time: Some(LocalTime { year: 2005, ..TIME }),
        ..subject("a", "")
    };
    assert_eq!(mask("[y]").render(&s, 0), "05");
}

#[test]
fn upper_applies_to_what_follows() {
    assert_eq!(render("[N1][U][N2-3]-x", "abcd", ""), "aBC-X");
}

#[test]
fn lower_applies_to_what_follows() {
    assert_eq!(render("[L][N]-[E]", "ABC", "JPG"), "abc-jpg");
}

#[test]
fn original_ends_a_case_run() {
    assert_eq!(render("[U][N1][n][N2-]", "abc", ""), "Abc");
    assert_eq!(render("[U]a[L]B[n]C", "x", ""), "AbC");
}

#[test]
fn a_later_case_token_replaces_the_earlier_one() {
    assert_eq!(render("[U][N][L][N]", "Ab", ""), "ABab");
}

#[test]
fn case_before_any_text_changes_nothing_by_itself() {
    assert_eq!(render("[U]", "abc", ""), "");
    assert_eq!(render("[N][U]", "abc", ""), "abc");
}

#[test]
fn case_mapping_handles_multi_byte_letters() {
    assert_eq!(render("[U][N]", "straße ñ", ""), "STRASSE Ñ");
    assert_eq!(render("[L][N]", "ÀÉ", ""), "àé");
}

#[test]
fn words_start_after_space_underscore_dash_and_dot() {
    assert_eq!(
        render("[F][N]", "my big_file-name.v2 final", ""),
        "My Big_File-Name.V2 Final"
    );
}

#[test]
fn words_keep_the_rest_of_each_word() {
    assert_eq!(render("[F][N]", "abcDEF ghi", ""), "AbcDEF Ghi");
}

#[test]
fn words_continue_across_tokens() {
    assert_eq!(render("[F]ab [N]", "cd", ""), "Ab Cd");
    assert_eq!(render("[F][N1][N2]", "ab", ""), "Ab");
}

#[test]
fn words_apply_to_the_extension() {
    assert_eq!(render("[F][N] [E]", "a", "txt"), "A Txt");
}

fn lookup(group: &str, field: &str) -> Option<String> {
    match (group, field) {
        ("exif", "date") => Some("2024-05-06".to_owned()),
        ("audio", "artist") => Some("Zoë".to_owned()),
        _ => None,
    }
}

fn with_fields(text: &str) -> String {
    let s = Subject {
        fields: Some(&lookup),
        ..subject("a", "")
    };
    mask(text).render(&s, 0)
}

#[test]
fn fields_come_from_the_lookup() {
    assert_eq!(
        with_fields("[=exif.date] [=audio.artist]"),
        "2024-05-06 Zoë"
    );
}

#[test]
fn a_missing_field_is_empty() {
    assert_eq!(with_fields("a[=file.size]b"), "ab");
}

#[test]
fn fields_are_empty_without_a_lookup() {
    assert_eq!(render("a[=exif.date]b", "x", ""), "ab");
}

#[test]
fn fields_follow_the_case_mode() {
    assert_eq!(with_fields("[U][=audio.artist]"), "ZOË");
}

#[test]
fn parse_builds_field_tokens() {
    assert_eq!(
        mask("[=exif.date_2]").tokens(),
        [Token::Field {
            group: "exif".into(),
            field: "date_2".into()
        }]
    );
}

#[test]
fn malformed_fields_are_errors() {
    for text in [
        "[=]",
        "[=exif]",
        "[=exif.]",
        "[=.date]",
        "[=Exif.date]",
        "[=exif.da-te]",
        "[=a.b.c]",
        "[=exif date]",
        "[= exif.date]",
        "[=é.x]",
    ] {
        let err = error(text);
        assert_eq!((err.kind, err.at), (MaskErrorKind::BadField, 1), "{text}");
    }
}

#[test]
fn doubled_brackets_are_literal() {
    assert_eq!(render("[[N]]", "abc", ""), "[N]");
    assert_eq!(render("[[[N]]]", "abc", ""), "[abc]");
    assert_eq!(render("a[[b", "x", ""), "a[b");
    assert_eq!(render("a]]b", "x", ""), "a]b");
}

#[test]
fn text_and_tokens_mix_in_order() {
    assert_eq!(
        render("IMG_[C1+1:3]_[N1-3].[E]", "holiday", "JPG"),
        "IMG_001_hol.JPG"
    );
}

#[test]
fn token_letters_are_case_sensitive() {
    assert_eq!(render("[M][i]", "a", ""), "0305");
    assert_eq!(render("[N][n]", "Ab", ""), "Ab");
    assert_eq!(error("[m]").kind, MaskErrorKind::UnknownToken);
    assert_eq!(error("[p]").kind, MaskErrorKind::UnknownToken);
}

#[test]
fn unknown_token_reports_its_position() {
    assert_eq!(
        error("abc[Q]"),
        MaskError {
            at: 4,
            kind: MaskErrorKind::UnknownToken
        }
    );
}

#[test]
fn position_counts_characters_not_bytes() {
    assert_eq!(error("日本[X]").at, 3);
}

#[test]
fn position_of_a_later_token_is_its_own_bracket() {
    assert_eq!(error("[N][E][bogus]").at, 7);
}

#[test]
fn empty_brackets_are_unknown() {
    assert_eq!(error("[]").kind, MaskErrorKind::UnknownToken);
}

#[test]
fn fixed_tokens_take_no_arguments() {
    for text in ["[P1]", "[G2]", "[Y4]", "[M-1]", "[D ]", "[ N]"] {
        assert_eq!(error(text).kind, MaskErrorKind::UnknownToken, "{text}");
    }
}

#[test]
fn unclosed_bracket_is_reported_at_the_bracket() {
    assert_eq!(
        error("ab[N2-5"),
        MaskError {
            at: 3,
            kind: MaskErrorKind::Unclosed
        }
    );
    assert_eq!(error("[").kind, MaskErrorKind::Unclosed);
    assert_eq!(error("[[[").kind, MaskErrorKind::Unclosed);
}

#[test]
fn single_closing_bracket_is_an_error() {
    assert_eq!(
        error("ab]c"),
        MaskError {
            at: 3,
            kind: MaskErrorKind::StrayClose
        }
    );
    assert_eq!(error("[N]]").kind, MaskErrorKind::StrayClose);
}

#[test]
fn malformed_spans_are_errors() {
    for text in [
        "[N2--]",
        "[N2-3-4]",
        "[N2,]",
        "[N,3]",
        "[N2,-3]",
        "[Nx]",
        "[N2x]",
        "[N-]",
        "[N--2]",
        "[N+2]",
        "[N99999999999]",
        "[N2,99999999999]",
    ] {
        let err = error(text);
        assert_eq!((err.kind, err.at), (MaskErrorKind::BadSpan, 1), "{text}");
    }
}

#[test]
fn malformed_counters_are_errors() {
    for text in [
        "[C10]",
        "[C10+5]",
        "[C+5:3]",
        "[C10+5:]",
        "[C10+0:3]",
        "[C1+1:0]",
        "[C1+1:11]",
        "[C1+1:3x]",
        "[Cx]",
        "[C-1+1:3]",
        "[C99999999999999999999+1:1]",
    ] {
        assert_eq!(error(text).kind, MaskErrorKind::BadCounter, "{text}");
    }
}

#[test]
fn counter_digits_one_to_ten_are_accepted() {
    assert!(Mask::parse("[C1+1:1]").is_ok());
    assert!(Mask::parse("[C1+1:10]").is_ok());
}

#[test]
fn parse_builds_the_expected_tokens() {
    assert_eq!(
        mask("a[N2-4]b[C]").tokens(),
        [
            Token::Text("a".into()),
            Token::Name(Span::Range(2, 4)),
            Token::Text("b".into()),
            Token::Counter(None),
        ]
    );
}

const TABLE: &[&str] = &[
    "[N]",
    "[N3]",
    "[N2-5]",
    "[N2,5]",
    "[N2-]",
    "[N-3]",
    "[N-3,3]",
    "[N2--2]",
    "[N-4--2]",
    "[N-2-]",
    "[E]",
    "[E3]",
    "[E2-5]",
    "[E2,5]",
    "[E2-]",
    "[E-3,3]",
    "[P]",
    "[G]",
    "[C]",
    "[C10+5:3]",
    "[Y]",
    "[y]",
    "[M]",
    "[D]",
    "[h]",
    "[i]",
    "[s]",
    "[d]",
    "[t]",
    "[U]",
    "[L]",
    "[F]",
    "[n]",
    "[=exif.date]",
    "[=audio.artist]",
    "[=file.size]",
    "[=exif]",
    "[=Exif.date]",
    "[[",
    "]]",
    "a[[N]]b",
    "[Q]",
    "[N2--]",
    "[C0+0:3]",
    "x[",
    "x]",
    "[N2-5",
];

#[test]
fn token_table() {
    let lines: Vec<String> = TABLE
        .iter()
        .map(|text| match Mask::parse(text) {
            Ok(m) => format!("{text} => {:?}", m.tokens()),
            Err(e) => format!("{text} => error: {e:?}"),
        })
        .collect();
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("mask_token_table", lines.join("\n"));
    });
}
