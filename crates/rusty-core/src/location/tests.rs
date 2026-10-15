use super::*;

#[test]
fn keeps_the_text_unchanged() {
    let text = "C:\\Users\\admin\\ \"quoted\" ünï 📁";
    assert_eq!(
        Location::new(text).map(|l| l.as_str().to_owned()),
        Ok(text.to_owned())
    );
}

#[test]
fn rejects_empty_text() {
    assert_eq!(Location::new(""), Err(LocationError::Empty));
}

#[test]
fn rejects_nul() {
    assert_eq!(Location::new("a\0b"), Err(LocationError::Nul));
}
