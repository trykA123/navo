//! Locates the items that `#[cfg(test)]` makes test-only and blanks comments
//! and literals, so they hide no braces or channel patterns.

/// The `#[cfg(test)]` attribute this scanner looks for.
const CFG_TEST: &str = "#[cfg(test)]";

/// Structural tokens the test-skip scan needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    CfgTest,
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    Comma,
    Semi,
}

/// One structural token at a 1-based line.
#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    line: usize,
}

/// The tokens of one file and, per line, its code with comments and literals
/// blanked out.
struct Lexed {
    tokens: Vec<Token>,
    code: Vec<String>,
}

/// Test-only lines and per-line code of one source file.
pub(super) struct Scanned {
    /// True per 1-based line that belongs to test-only code.
    pub(super) is_test: Vec<bool>,
    /// The file's code per line, with comments and literals blanked out.
    pub(super) code: Vec<String>,
}

/// Scan `text` for test-only lines and for countable code.
pub(super) fn scanned(text: &str) -> Scanned {
    let lexed = lex(text);
    let is_test = test_lines(&lexed.tokens, lexed.code.len());
    Scanned {
        is_test,
        code: lexed.code,
    }
}

/// Split `text` into structural tokens and per-line code.
///
/// Comments and literals are blanked out of `code`, so their braces are not
/// structural and their text cannot look like a channel construction.
fn lex(text: &str) -> Lexed {
    let mut lexed = Lexed {
        tokens: Vec::new(),
        code: vec![String::new()],
    };
    let mut line = 1usize;
    let mut index = 0usize;
    while index < text.len() {
        let rest = &text[index..];
        let Some(first) = rest.chars().next() else {
            break;
        };
        if first == '\n' {
            lexed.code.push(String::new());
            line += 1;
            index += 1;
            continue;
        }
        if rest.starts_with(CFG_TEST) {
            lexed.tokens.push(Token {
                kind: Kind::CfgTest,
                line,
            });
            let end = index + CFG_TEST.len();
            consume(text, index, end, &mut line, &mut lexed.code);
            index = end;
            continue;
        }
        if let Some(end) = non_code_end(text, index) {
            consume(text, index, end, &mut line, &mut lexed.code);
            index = end;
            continue;
        }
        if let Some(kind) = punctuation(first) {
            lexed.tokens.push(Token { kind, line });
        }
        if let Some(current) = lexed.code.last_mut() {
            current.push(first);
        }
        index += first.len_utf8();
    }
    lexed
}

/// End of a comment or literal starting at `start`, if one does.
///
/// `None` means the character is ordinary code, including a lifetime quote.
fn non_code_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    if rest.starts_with("//") {
        return Some(
            text[start..]
                .find('\n')
                .map_or(text.len(), |offset| start + offset),
        );
    }
    if rest.starts_with("/*") {
        return Some(block_comment_end(text, start));
    }
    if rest.starts_with('"') {
        return Some(string_end(text, start));
    }
    if rest.starts_with('\'') {
        return char_literal_end(text, start);
    }
    raw_string_end(text, start)
}

/// Blank a non-code span and record every newline it holds.
fn consume(text: &str, from: usize, to: usize, line: &mut usize, code: &mut Vec<String>) {
    if let Some(current) = code.last_mut() {
        current.push(' ');
    }
    for character in text[from..to].chars() {
        if character == '\n' {
            code.push(String::new());
            *line += 1;
        }
    }
}

/// End of the block comment starting at `start`, honouring nesting.
fn block_comment_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 1usize;
    let mut at = start + 2;
    while at < bytes.len() {
        if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'*') {
            depth += 1;
            at += 2;
        } else if bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/') {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    bytes.len()
}

/// End of the plain string literal starting at `start`.
fn string_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
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

/// End of the raw string literal at `start`, if that is what starts there.
fn raw_string_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'r') {
        return None;
    }
    let hashes = bytes[start + 1..]
        .iter()
        .take_while(|&&byte| byte == b'#')
        .count();
    if bytes.get(start + 1 + hashes) != Some(&b'"') {
        return None;
    }
    let mut at = start + hashes + 2;
    while at < bytes.len() {
        let closes = bytes[at] == b'"'
            && bytes
                .get(at + 1..at + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|&byte| byte == b'#'));
        if closes {
            return Some(at + 1 + hashes);
        }
        at += 1;
    }
    Some(bytes.len())
}

/// End of the char literal at `start`, if the quote opens one and not a lifetime.
fn char_literal_end(text: &str, start: usize) -> Option<usize> {
    let mut characters = text[start + 1..].char_indices();
    let (_, first) = characters.next()?;
    if first == '\\' {
        characters.next()?;
        let (offset, _) = characters.find(|(_, character)| *character == '\'')?;
        return Some(start + 1 + offset + 1);
    }
    let (offset, second) = characters.next()?;
    (second == '\'').then_some(start + 1 + offset + 1)
}

/// The structural kind of `character`, if it is one this scan tracks.
fn punctuation(character: char) -> Option<Kind> {
    match character {
        '{' => Some(Kind::OpenBrace),
        '}' => Some(Kind::CloseBrace),
        '(' => Some(Kind::OpenParen),
        ')' => Some(Kind::CloseParen),
        ',' => Some(Kind::Comma),
        ';' => Some(Kind::Semi),
        _ => None,
    }
}

/// Return, per 1-based line, whether the line is test-only code.
fn test_lines(tokens: &[Token], line_count: usize) -> Vec<bool> {
    let mut is_test = vec![false; line_count];
    let mut paren = 0usize;
    let mut brace = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        if token.kind != Kind::CfgTest {
            apply_depth(token.kind, &mut paren, &mut brace);
            index += 1;
            continue;
        }
        let end = item_end(tokens, index + 1, paren, brace);
        let last = tokens
            .get(end.saturating_sub(1))
            .map_or(token.line, |item| item.line);
        for line in token.line..=last {
            if let Some(flag) = is_test.get_mut(line - 1) {
                *flag = true;
            }
        }
        for skipped in &tokens[index..end] {
            apply_depth(skipped.kind, &mut paren, &mut brace);
        }
        index = end.max(index + 1);
    }
    is_test
}

/// Return the token index just past the item a `#[cfg(test)]` applies to.
///
/// `paren` and `brace` are the depths at the attribute. A comma at those
/// depths ends a field, variant, match arm or parameter, but only inside a
/// block: a top-level item ends at its `;` or at its balanced `{ ... }`.
fn item_end(tokens: &[Token], start: usize, paren: usize, brace: usize) -> usize {
    let mut paren_depth = paren;
    let mut brace_depth = brace;
    for (offset, token) in tokens[start..].iter().enumerate() {
        match token.kind {
            Kind::OpenParen => paren_depth += 1,
            Kind::CloseParen => {
                if paren_depth <= paren {
                    return start + offset + 1;
                }
                paren_depth -= 1;
            }
            Kind::OpenBrace => brace_depth += 1,
            Kind::CloseBrace => {
                if brace_depth <= brace {
                    return start + offset;
                }
                brace_depth -= 1;
                if brace_depth == brace && paren_depth == paren {
                    return start + offset + 1;
                }
            }
            Kind::Comma if paren_depth == paren && brace_depth == brace && brace > 0 => {
                return start + offset + 1;
            }
            Kind::Semi if paren_depth == paren && brace_depth == brace => {
                return start + offset + 1;
            }
            Kind::CfgTest | Kind::Comma | Kind::Semi => {}
        }
    }
    tokens.len()
}

/// Track the paren and brace depths across one token.
fn apply_depth(kind: Kind, paren: &mut usize, brace: &mut usize) {
    match kind {
        Kind::OpenParen => *paren += 1,
        Kind::CloseParen => *paren = paren.saturating_sub(1),
        Kind::OpenBrace => *brace += 1,
        Kind::CloseBrace => *brace = brace.saturating_sub(1),
        Kind::CfgTest | Kind::Comma | Kind::Semi => {}
    }
}
