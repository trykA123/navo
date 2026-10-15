use super::{NameError, check_name};

#[test]
fn ordinary_names_pass() {
    for name in ["notes.txt", "Q3 report", "ăîșț", "a.b.c", ".gitignore", "x"] {
        assert_eq!(check_name(name), Ok(()), "{name}");
    }
}

#[test]
fn an_empty_or_blank_name_is_refused() {
    assert_eq!(check_name(""), Err(NameError::Empty));
    assert_eq!(check_name("   "), Err(NameError::Empty));
}

#[test]
fn reserved_characters_are_named() {
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        let name = format!("a{ch}b");
        assert_eq!(check_name(&name), Err(NameError::Character(ch)), "{name}");
    }
    assert_eq!(check_name("a\u{1}b"), Err(NameError::Character('\u{1}')));
}

#[test]
fn dot_names_are_refused() {
    assert_eq!(check_name("."), Err(NameError::DotName));
    assert_eq!(check_name(".."), Err(NameError::DotName));
}

#[test]
fn a_trailing_space_or_dot_is_refused() {
    assert_eq!(check_name("draft "), Err(NameError::Trailing));
    assert_eq!(check_name("draft."), Err(NameError::Trailing));
}

#[test]
fn windows_device_names_are_refused() {
    for name in ["CON", "prn", "aux.txt", "NUL", "com1", "LPT9.log"] {
        assert_eq!(check_name(name), Err(NameError::Reserved), "{name}");
    }
    assert_eq!(check_name("console"), Ok(()));
}

#[test]
fn every_error_reads_as_a_sentence() {
    assert_eq!(NameError::Empty.to_string(), "Type a name.");
    assert_eq!(
        NameError::Character('?').to_string(),
        "A name cannot contain ?"
    );
}
