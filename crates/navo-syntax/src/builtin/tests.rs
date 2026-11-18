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
fn builtin_has_a2l_dbc_dcm_ldf_lsl_m4_map_and_oil() {
    let mut names: Vec<_> = builtin().into_iter().map(|l| l.name).collect();
    names.sort();
    assert_eq!(
        names,
        ["a2l", "dbc", "dcm", "ldf", "lsl", "m4", "map", "oil"]
    );
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

#[test]
fn a2l_sample_gives_expected_kinds() {
    let a2l = language("a2l");
    let start = r#"/begin MEASUREMENT EngSpeed "Engine speed" UWORD ECU_ADDRESS 0x1234 // rpm"#;
    assert_eq!(kind_of(&a2l, start, "/begin"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&a2l, start, "MEASUREMENT"), Some(TokenKind::Type));
    assert_eq!(
        kind_of(&a2l, start, r#""Engine speed""#),
        Some(TokenKind::String)
    );
    assert_eq!(kind_of(&a2l, start, "UWORD"), Some(TokenKind::Constant));
    assert_eq!(kind_of(&a2l, start, "// rpm"), Some(TokenKind::Comment));
    let end = "/end MEASUREMENT /* done */";
    assert_eq!(kind_of(&a2l, end, "/end"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&a2l, end, "/* done */"), Some(TokenKind::Comment));
}

#[test]
fn dbc_sample_gives_expected_kinds() {
    let dbc = language("dbc");
    let frame = "BO_ 1234 EngineData: 8 ECU";
    assert_eq!(kind_of(&dbc, frame, "BO_"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&dbc, frame, ":"), Some(TokenKind::Operator));
    let signal = r#" SG_ EngineSpeed : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX"#;
    assert_eq!(kind_of(&dbc, signal, "SG_"), Some(TokenKind::Keyword));
    assert_eq!(
        kind_of(&dbc, signal, "Vector__XXX"),
        Some(TokenKind::Constant)
    );
    assert_eq!(kind_of(&dbc, signal, r#""rpm""#), Some(TokenKind::String));
    assert_eq!(
        kind_of(&dbc, "// engine speed", "// engine speed"),
        Some(TokenKind::Comment)
    );
}

#[test]
fn ldf_sample_gives_expected_kinds() {
    let ldf = language("ldf");
    let speed = r#"LIN_speed = 19.2 kbps; // LIN 2.1"#;
    assert_eq!(kind_of(&ldf, speed, "LIN_speed"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&ldf, speed, "="), Some(TokenKind::Operator));
    assert_eq!(kind_of(&ldf, speed, "kbps"), Some(TokenKind::Type));
    assert_eq!(kind_of(&ldf, speed, "19.2"), Some(TokenKind::Number));
    assert_eq!(kind_of(&ldf, speed, "// LIN 2.1"), Some(TokenKind::Comment));
    let frames = "Frames { MasterReq: 0x3C, SlaveResp: 0x3D } /* header */";
    assert_eq!(kind_of(&ldf, frames, "Frames"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&ldf, frames, "MasterReq"), Some(TokenKind::Keyword));
    assert_eq!(
        kind_of(&ldf, frames, "/* header */"),
        Some(TokenKind::Comment)
    );
}

#[test]
fn lsl_sample_gives_expected_kinds() {
    let lsl = language("lsl");
    let include = r#"#include "reg.lsl" // register map"#;
    assert_eq!(kind_of(&lsl, include, "#include"), Some(TokenKind::Keyword));
    assert_eq!(
        kind_of(&lsl, include, r#""reg.lsl""#),
        Some(TokenKind::String)
    );
    assert_eq!(
        kind_of(&lsl, include, "// register map"),
        Some(TokenKind::Comment)
    );
    let layout = "section_layout linear { group (ordered, align = 4); } /* table */";
    assert_eq!(
        kind_of(&lsl, layout, "section_layout"),
        Some(TokenKind::Keyword)
    );
    assert_eq!(kind_of(&lsl, layout, "ordered"), Some(TokenKind::Keyword));
    assert_eq!(
        kind_of(&lsl, layout, "/* table */"),
        Some(TokenKind::Comment)
    );
}

#[test]
fn map_sample_gives_expected_kinds() {
    let map = language("map");
    let header = "Section | Address | Size | Name";
    assert_eq!(kind_of(&map, header, "Section"), Some(TokenKind::Keyword));
    assert_eq!(kind_of(&map, header, "|"), Some(TokenKind::Operator));
    assert_eq!(
        kind_of(&map, "Usage: 12.3 KiB", "Usage"),
        Some(TokenKind::Keyword)
    );
    assert_eq!(kind_of(&map, "Size + 4", "+"), Some(TokenKind::Operator));
    assert_eq!(
        kind_of(&map, "* total memory", "* total memory"),
        Some(TokenKind::Comment)
    );
}
