use super::*;

#[test]
fn highlight_names_are_ones_the_editor_theme_knows() {
    let expected = [
        (TokenKind::Keyword, "keyword"),
        (TokenKind::Type, "type"),
        (TokenKind::Function, "function"),
        (TokenKind::Constant, "constant"),
        (TokenKind::Attribute, "attribute"),
        (TokenKind::Tag, "tag"),
        (TokenKind::Property, "property"),
        (TokenKind::Variable, "variable.special"),
        (TokenKind::Comment, "comment"),
        (TokenKind::String, "string"),
        (TokenKind::Number, "number"),
        (TokenKind::Operator, "operator"),
        (TokenKind::Delimiter, "punctuation.delimiter"),
    ];
    let actual: Vec<_> = expected
        .iter()
        .map(|(kind, _)| (*kind, kind.highlight_name()))
        .collect();
    assert_eq!(actual, expected);
}
