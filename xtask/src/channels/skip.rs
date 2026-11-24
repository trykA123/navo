//! Locates the items that `#[cfg(test)]` makes test-only and blanks them along
//! with comments and literals, so only countable code remains.

/// Structural tokens the test-skip scan needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    CfgTest,
    InnerCfgTest,
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    Comma,
    Semi,
}

/// One structural token over a byte range of the source.
#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// The tokens of one file and the byte ranges that hold no code.
struct Lexed {
    tokens: Vec<Token>,
    hidden: Vec<(usize, usize)>,
}

/// Return `text` with test-only items, comments and literals blanked out.
///
/// Only the test item itself is blanked, so a live call on the same line as a
/// test attribute is still read.
pub(super) fn masked(text: &str) -> String {
    let lexed = lex(text);
    let mut hidden = lexed.hidden;
    hidden.extend(test_ranges(text, &lexed.tokens));
    blank(text, hidden)
}

/// Tokenise `text`, collecting the byte ranges that hold no code.
fn lex(text: &str) -> Lexed {
    let mut lexed = Lexed {
        tokens: Vec::new(),
        hidden: Vec::new(),
    };
    let mut index = 0usize;
    while index < text.len() {
        if let Some((end, is_test, inner)) = attribute(text, index) {
            push_attribute(&mut lexed, index, end, is_test, inner);
            index = end;
            continue;
        }
        if let Some(end) = non_code_end(text, index) {
            lexed.hidden.push((index, end));
            index = end;
            continue;
        }
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        if let Some(kind) = punctuation(character) {
            lexed.tokens.push(Token {
                kind,
                start: index,
                end: index + 1,
            });
        }
        index += character.len_utf8();
    }
    lexed
}

/// Record an attribute: always hidden, and a token when it is test-only.
fn push_attribute(lexed: &mut Lexed, start: usize, end: usize, is_test: bool, inner: bool) {
    if is_test {
        let kind = if inner {
            Kind::InnerCfgTest
        } else {
            Kind::CfgTest
        };
        lexed.tokens.push(Token { kind, start, end });
    }
    lexed.hidden.push((start, end));
}

/// End, test-only verdict and inner flag of the attribute at `start`, if any.
fn attribute(text: &str, start: usize) -> Option<(usize, bool, bool)> {
    let (bracket, inner) = match &text.as_bytes()[start..] {
        [b'#', b'[', ..] => (start + 1, false),
        [b'#', b'!', b'[', ..] => (start + 2, true),
        _ => return None,
    };
    let end = attribute_end(text, bracket);
    Some((end, test_only_cfg(&text[start..end]), inner))
}

/// End of the attribute whose `[` is at `bracket`, honouring nested brackets.
fn attribute_end(text: &str, bracket: usize) -> usize {
    let mut depth = 1usize;
    let mut at = bracket + 1;
    while at < text.len() {
        if let Some(end) = non_code_end(text, at) {
            at = end;
            continue;
        }
        let Some(character) = text[at..].chars().next() else {
            break;
        };
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += character.len_utf8();
    }
    text.len()
}

/// True when the attribute is compiled only in tests.
///
/// `test`, `#[cfg( test )]` and any `all(...)` with a test-only conjunct are
/// test-only. `any(...)` and `not(...)` stay live: they can build without
/// `test`, so their calls are counted rather than hidden.
fn test_only_cfg(attribute: &str) -> bool {
    let inner = attribute
        .trim_start_matches('#')
        .trim_start_matches('!')
        .trim();
    let Some(cfg) = inner
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .map(str::trim)
        .and_then(|rest| rest.strip_prefix("cfg"))
        .map(str::trim_start)
    else {
        return false;
    };
    let Some(expr) = cfg
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    cfg_test_only(expr.trim())
}

/// True when `expr` is true only under `cfg(test)`.
fn cfg_test_only(expr: &str) -> bool {
    if expr == "test" {
        return true;
    }
    let Some(inner) = strip_call(expr, "all") else {
        return false;
    };
    split_args(inner)
        .iter()
        .any(|argument| cfg_test_only(argument.trim()))
}

/// The arguments of `name(...)`, when `expr` is exactly that call.
fn strip_call<'a>(expr: &'a str, name: &str) -> Option<&'a str> {
    let rest = expr.strip_prefix(name)?.trim_start();
    rest.strip_prefix('(')?.strip_suffix(')')
}

/// Split comma-separated `cfg` arguments at the top level.
fn split_args(inner: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0usize;
    for (at, character) in inner.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    if start < inner.len() {
        args.push(&inner[start..]);
    }
    args
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

/// Return the byte ranges of the items that `#[cfg(test)]` makes test-only.
fn test_ranges(text: &str, tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut paren = 0usize;
    let mut brace = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        if token.kind == Kind::InnerCfgTest {
            let (next, start, end) = inner_range(tokens, index, brace);
            ranges.push((start, end));
            for skipped in &tokens[index..next] {
                apply_depth(skipped.kind, &mut paren, &mut brace);
            }
            index = next.max(index + 1);
            continue;
        }
        if token.kind != Kind::CfgTest {
            apply_depth(token.kind, &mut paren, &mut brace);
            index += 1;
            continue;
        }
        let (next, end) = item_end(text, tokens, index + 1, paren, brace);
        ranges.push((token.start, end.max(token.end)));
        for skipped in &tokens[index..next] {
            apply_depth(skipped.kind, &mut paren, &mut brace);
        }
        index = next.max(index + 1);
    }
    ranges
}

/// Range and next token index of the file or block `#![cfg(test)]` covers.
fn inner_range(tokens: &[Token], index: usize, brace: usize) -> (usize, usize, usize) {
    if brace == 0 {
        return (
            tokens.len(),
            0,
            tokens.last().map_or(tokens[index].end, |token| token.end),
        );
    }
    let start = enclosing_brace(tokens, index).unwrap_or(0);
    let mut depth = brace;
    for (offset, token) in tokens[index + 1..].iter().enumerate() {
        match token.kind {
            Kind::OpenBrace => depth += 1,
            Kind::CloseBrace => {
                depth -= 1;
                if depth < brace {
                    return (index + offset + 2, start, token.end);
                }
            }
            _ => {}
        }
    }
    let end = tokens.last().map_or(start, |token| token.end);
    (tokens.len(), start, end)
}

/// Start of the brace block that encloses the token at `index`.
fn enclosing_brace(tokens: &[Token], index: usize) -> Option<usize> {
    let mut balance = 0usize;
    for token in tokens[..index].iter().rev() {
        match token.kind {
            Kind::CloseBrace => balance += 1,
            Kind::OpenBrace if balance == 0 => return Some(token.start),
            Kind::OpenBrace => balance -= 1,
            _ => {}
        }
    }
    None
}

/// Return the token index just past the item and its final byte offset.
///
/// `paren` and `brace` are the depths at the attribute. A comma at those
/// depths ends a field, variant, match arm or parameter, but only inside a
/// block: a top-level item ends at its `;` or at its body `{ ... }`.
fn item_end(
    text: &str,
    tokens: &[Token],
    start: usize,
    paren: usize,
    brace: usize,
) -> (usize, usize) {
    let mut paren_depth = paren;
    let mut brace_depth = brace;
    let mut body = false;
    for (offset, token) in tokens[start..].iter().enumerate() {
        match token.kind {
            Kind::OpenParen => paren_depth += 1,
            Kind::CloseParen => {
                if paren_depth <= paren {
                    return (start + offset + 1, token.end);
                }
                paren_depth -= 1;
            }
            Kind::OpenBrace => {
                brace_depth += 1;
                if brace_depth == brace + 1 {
                    body = is_body_brace(text, token.start);
                }
            }
            Kind::CloseBrace => {
                if brace_depth <= brace {
                    return (start + offset, token.start);
                }
                brace_depth -= 1;
                if brace_depth == brace && paren_depth == paren && body {
                    return (start + offset + 1, token.end);
                }
                if brace_depth == brace {
                    body = false;
                }
            }
            Kind::Comma if paren_depth == paren && brace_depth == brace && brace > 0 => {
                return (start + offset + 1, token.end);
            }
            Kind::Semi if paren_depth == paren && brace_depth == brace => {
                return (start + offset + 1, token.end);
            }
            Kind::CfgTest | Kind::InnerCfgTest | Kind::Comma | Kind::Semi => {}
        }
    }
    (tokens.len(), tokens.last().map_or(0, |token| token.end))
}

/// False for a brace group in a generic argument list (`Trait<{1}>`).
///
/// If the heuristic is ever wrong it ends the skip early, which counts a call
/// rather than hiding it.
fn is_body_brace(text: &str, at: usize) -> bool {
    !matches!(
        text[..at]
            .chars()
            .rev()
            .find(|character| !character.is_whitespace()),
        Some('<')
    )
}

/// Track the paren and brace depths across one token.
fn apply_depth(kind: Kind, paren: &mut usize, brace: &mut usize) {
    match kind {
        Kind::OpenParen => *paren += 1,
        Kind::CloseParen => *paren = paren.saturating_sub(1),
        Kind::OpenBrace => *brace += 1,
        Kind::CloseBrace => *brace = brace.saturating_sub(1),
        Kind::CfgTest | Kind::InnerCfgTest | Kind::Comma | Kind::Semi => {}
    }
}

/// Blank every hidden range, keeping newlines so line numbers do not move.
fn blank(text: &str, mut ranges: Vec<(usize, usize)>) -> String {
    ranges.retain(|(start, end)| start < end);
    ranges.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for (start, end) in ranges {
        if end <= last {
            continue;
        }
        if start > last {
            out.push_str(&text[last..start]);
        }
        let from = start.max(last);
        for character in text[from..end].chars() {
            out.push(if character == '\n' { '\n' } else { ' ' });
        }
        last = end;
    }
    out.push_str(&text[last..]);
    out
}
