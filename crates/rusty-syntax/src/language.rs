use crate::TokenKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub name: String,
    pub extensions: Vec<String>,
    pub case_ignored: bool,
    pub keywords: Vec<KeywordGroup>,
    pub line_comments: Vec<String>,
    pub block_comments: Vec<(String, String)>,
    pub delimiters: Vec<Delimiter>,
    pub operators: Vec<String>,
    pub numbers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordGroup {
    pub kind: TokenKind,
    pub words: Vec<String>,
    pub prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delimiter {
    pub open: String,
    pub close: String,
    pub escape: Option<char>,
    pub multiline: bool,
    pub kind: TokenKind,
}
