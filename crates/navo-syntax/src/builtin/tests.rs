use super::*;
use crate::{LineState, TokenKind, lex_line};

fn language(name: &str) -> Language {
    let Some(lang) = builtin().into_iter().find(|l| l.name == name) else {
        panic!("builtin language {name} is missing");
    };
    lang
}

fn kind_of(lang: &Language, line: &str, text: &str) -> Option<TokenKind> {
    let (tokens, _) = lex_line(lang, line, LineState::Normal);
    tokens
        .into_iter()
        .find(|t| line.get(t.range.clone()) == Some(text))
        .map(|t| t.kind)
}

#[test]
fn builtin_has_m4_oil_and_dcm() {
    let mut names: Vec<_> = builtin().into_iter().map(|l| l.name).collect();
    names.sort();
    assert_eq!(names, ["dcm", "m4", "oil"]);
}

#[test]
fn m4_sample_gives_expected_kinds() {
    let m4 = language("m4");
    let line = "define(`GREETING', `hello world')dnl set the name";
    assert_eq!(kind_of(&m4, line, "define"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&m4, line, "`GREETING'"), Some(TokenKind::String));
    assert_eq!(kind_of(&m4, line, "`hello world'"), Some(TokenKind::String));
    assert_eq!(
        kind_of(&m4, line, "dnl set the name"),
        Some(TokenKind::Comment)
    );
    assert_eq!(
        kind_of(&m4, "# hash comment", "# hash comment"),
        Some(TokenKind::Comment)
    );
}

#[test]
fn oil_sample_gives_expected_kinds() {
    let oil = language("oil");
    assert_eq!(kind_of(&oil, "CPU ecu {", "CPU"), Some(TokenKind::Keyword));
    let task = "    TASK Init { PRIORITY = 1; AUTOSTART = FALSE; }; // boot";
    assert_eq!(kind_of(&oil, task, "TASK"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&oil, task, "1"), Some(TokenKind::Number));
    assert_eq!(kind_of(&oil, task, "FALSE"), Some(TokenKind::Constant));
    assert_eq!(kind_of(&oil, task, "// boot"), Some(TokenKind::Comment));
    let alarm = "    ALARM Tick { AUTOSTART = TRUE; }; /* every 10 ms */";
    assert_eq!(kind_of(&oil, alarm, "ALARM"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&oil, alarm, "TRUE"), Some(TokenKind::Constant));
    assert_eq!(
        kind_of(&oil, alarm, "/* every 10 ms */"),
        Some(TokenKind::Comment)
    );
}

#[test]
fn dcm_sample_gives_expected_kinds() {
    let dcm = language("dcm");
    let format = "KONSERVIERUNG_FORMAT 2.0";
    assert_eq!(
        kind_of(&dcm, format, "KONSERVIERUNG_FORMAT"),
        Some(TokenKind::Keyword)
    );
    assert_eq!(kind_of(&dcm, format, "2.0"), Some(TokenKind::Number));
    assert_eq!(
        kind_of(&dcm, "* exported from INCA", "* exported from INCA"),
        Some(TokenKind::Comment)
    );
    assert_eq!(
        kind_of(&dcm, "FESTWERT Ign_Offset", "FESTWERT"),
        Some(TokenKind::Keyword)
    );
    let curve = "KENNLINIE Idle_Speed 3";
    assert_eq!(kind_of(&dcm, curve, "KENNLINIE"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&dcm, curve, "3"), Some(TokenKind::Number));
    let values = "   WERT 1100.0 900.0 750.0";
    assert_eq!(kind_of(&dcm, values, "WERT"), Some(TokenKind::Property));
    assert_eq!(kind_of(&dcm, values, "900.0"), Some(TokenKind::Number));
    assert_eq!(kind_of(&dcm, "END", "END"), Some(TokenKind::Keyword));
}
