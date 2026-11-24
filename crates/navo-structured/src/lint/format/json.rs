//! JSON pretty-printer: token walker that keeps key order and number text verbatim.

use crate::lint::split_bom;

/// One JSON token as written in the source.
enum Token<'a> {
    Str(&'a str),
    Word(&'a str),
    Open(char),
    Close(char),
    Colon,
    Comma,
}

/// Mutable output state: text written so far and the open containers.
struct Layout {
    out: String,
    stack: Vec<char>,
}

impl Layout {
    /// Writes an opening bracket and returns how many tokens it consumed.
    fn open(&mut self, tokens: &[Token<'_>], at: usize, open: char) -> usize {
        let close = if open == '{' { '}' } else { ']' };
        self.out.push(open);
        if matches!(tokens.get(at + 1), Some(Token::Close(found)) if *found == close) {
            self.out.push(close);
            return 2;
        }
        self.line(self.stack.len() + 1);
        self.stack.push(open);
        1
    }

    /// Writes a closing bracket on its own line at the parent indent.
    fn close(&mut self, close: char) {
        self.stack.pop();
        self.line(self.stack.len());
        self.out.push(close);
    }

    /// Starts a line indented by `level` two-space units.
    fn line(&mut self, level: usize) {
        self.out.push('\n');
        for _ in 0..level {
            self.out.push_str("  ");
        }
    }
}

/// Returns `text` re-indented with two spaces per level and a final line ending.
pub(super) fn format_json(text: &str) -> String {
    let (source, bom) = split_bom(text);
    let tokens = tokenize(source);
    let mut layout = Layout {
        out: String::new(),
        stack: Vec::new(),
    };
    let mut at = 0;
    while at < tokens.len() {
        match tokens[at] {
            Token::Open(open) => at += layout.open(&tokens, at, open),
            Token::Close(close) => {
                layout.close(close);
                at += 1;
            }
            Token::Str(value) | Token::Word(value) => {
                layout.out.push_str(value);
                at += 1;
            }
            Token::Colon => {
                layout.out.push_str(": ");
                at += 1;
            }
            Token::Comma => {
                layout.out.push(',');
                layout.line(layout.stack.len());
                at += 1;
            }
        }
    }
    super::finish(&layout.out, text, bom)
}

/// Splits `text` into JSON tokens, dropping whitespace outside strings.
fn tokenize(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut tokens = Vec::new();
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        match byte {
            b'"' => {
                let end = string_end(bytes, at);
                tokens.push(Token::Str(text.get(at..end).unwrap_or("")));
                at = end;
            }
            b'{' | b'[' => {
                tokens.push(Token::Open(char::from(byte)));
                at += 1;
            }
            b'}' | b']' => {
                tokens.push(Token::Close(char::from(byte)));
                at += 1;
            }
            b':' => {
                tokens.push(Token::Colon);
                at += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                at += 1;
            }
            _ => {
                let start = at;
                while at < bytes.len() && !is_delim(bytes[at]) {
                    at += 1;
                }
                tokens.push(Token::Word(text.get(start..at).unwrap_or("")));
            }
        }
    }
    tokens
}

/// Returns the byte just past the string that starts at `start`.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// Whether `byte` ends a number or keyword token.
fn is_delim(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'{' | b'}' | b'[' | b']' | b':' | b',' | b'"')
}
