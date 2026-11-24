//! Counts non-blank, non-comment product code lines in Rust source files,
//! skipping inline `#[cfg(test)]` and `#[test]` items.

#[derive(Default)]
enum Lexer {
    #[default]
    Code,
    BlockComment(usize),
    String,
    RawString(usize),
}

#[derive(Default)]
enum CfgTest {
    #[default]
    None,
    Pending {
        attr_brackets: usize,
    },
    InBlock(usize),
}

#[derive(Default)]
struct Counter {
    lexer: Lexer,
    cfg_test: CfgTest,
}

/// Count non-blank, non-comment product code lines in `source`.
///
/// Inline `#[cfg(test)]` and `#[test]` items are excluded so unit tests beside
/// the code do not count toward file size limits.
pub fn count_code_lines(source: &str) -> usize {
    let mut counter = Counter::default();
    source
        .lines()
        .filter(|line| counter.count_line(line))
        .count()
}

impl Counter {
    fn count_line(&mut self, line: &str) -> bool {
        let mut remaining = line.trim();
        let mut has_code = false;
        while !remaining.is_empty() {
            let (consumed, is_code) = self.consume(remaining);
            has_code |= is_code;
            remaining = &remaining[consumed..];
        }
        has_code
    }

    fn consume(&mut self, input: &str) -> (usize, bool) {
        match self.lexer {
            Lexer::Code => self.consume_code(input),
            Lexer::BlockComment(depth) => (self.consume_comment(input, depth), false),
            Lexer::String => (self.consume_string(input), self.in_product_code()),
            Lexer::RawString(hashes) => (
                self.consume_raw_string(input, hashes),
                self.in_product_code(),
            ),
        }
    }

    fn in_product_code(&self) -> bool {
        matches!(self.cfg_test, CfgTest::None)
    }

    fn consume_code(&mut self, input: &str) -> (usize, bool) {
        if input.starts_with("//") {
            return (input.len(), false);
        }
        if input.starts_with("/*") {
            self.lexer = Lexer::BlockComment(1);
            return (2, false);
        }
        if self.in_product_code()
            && let Some(length) = test_attribute_length(input)
        {
            self.cfg_test = CfgTest::Pending { attr_brackets: 0 };
            return (length, false);
        }
        if let Some((length, hashes)) = raw_string_start(input.as_bytes()) {
            self.lexer = Lexer::RawString(hashes);
            return (length, self.in_product_code());
        }
        if let Some(length) = char_literal_length(input) {
            return (length, self.in_product_code());
        }
        let Some(character) = input.chars().next() else {
            return (0, false);
        };
        if character == '"' {
            self.lexer = Lexer::String;
            return (1, self.in_product_code());
        }
        let is_code = self.in_product_code() && !character.is_whitespace();
        self.advance_cfg_test(character);
        (character.len_utf8(), is_code)
    }

    fn advance_cfg_test(&mut self, character: char) {
        match &mut self.cfg_test {
            CfgTest::None => {}
            CfgTest::Pending { attr_brackets } => match character {
                '[' => *attr_brackets += 1,
                ']' if *attr_brackets > 0 => *attr_brackets -= 1,
                ';' if *attr_brackets == 0 => self.cfg_test = CfgTest::None,
                '{' if *attr_brackets == 0 => self.cfg_test = CfgTest::InBlock(1),
                _ => {}
            },
            CfgTest::InBlock(depth) => match character {
                '{' => *depth += 1,
                '}' => {
                    *depth -= 1;
                    if *depth == 0 {
                        self.cfg_test = CfgTest::None;
                    }
                }
                _ => {}
            },
        }
    }

    fn consume_comment(&mut self, input: &str, depth: usize) -> usize {
        if input.starts_with("/*") {
            self.lexer = Lexer::BlockComment(depth + 1);
            return 2;
        }
        if input.starts_with("*/") {
            self.lexer = if depth == 1 {
                Lexer::Code
            } else {
                Lexer::BlockComment(depth - 1)
            };
            return 2;
        }
        input.chars().next().map_or(0, char::len_utf8)
    }

    fn consume_string(&mut self, input: &str) -> usize {
        match input.chars().next() {
            Some('\\') => input
                .chars()
                .nth(1)
                .map_or(1, |character| 1 + character.len_utf8()),
            Some('"') => {
                self.lexer = Lexer::Code;
                1
            }
            character => character.map_or(0, char::len_utf8),
        }
    }

    fn consume_raw_string(&mut self, input: &str, hashes: usize) -> usize {
        let closes = input.starts_with('"')
            && input
                .as_bytes()
                .get(1..hashes + 1)
                .is_some_and(|suffix| suffix.iter().all(|&byte| byte == b'#'));
        if closes {
            self.lexer = Lexer::Code;
            return hashes + 1;
        }
        input.chars().next().map_or(0, char::len_utf8)
    }
}

fn test_attribute_length(input: &str) -> Option<usize> {
    if !input.starts_with("#[") {
        return None;
    }
    let mut depth = 0usize;
    for (offset, character) in input[1..].char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let end = offset + 2;
                    let compact: String = input[..end]
                        .chars()
                        .filter(|c| !c.is_ascii_whitespace())
                        .collect();
                    return (compact == "#[cfg(test)]" || compact == "#[test]").then_some(end);
                }
            }
            _ => {}
        }
    }
    None
}

fn raw_string_start(input: &[u8]) -> Option<(usize, usize)> {
    if input.first() != Some(&b'r') {
        return None;
    }
    let hashes = input[1..].iter().take_while(|&&byte| byte == b'#').count();
    (input.get(hashes + 1) == Some(&b'"')).then_some((hashes + 2, hashes))
}

fn char_literal_length(input: &str) -> Option<usize> {
    if !input.starts_with('\'') {
        return None;
    }
    let mut characters = input[1..].char_indices();
    let (_, first) = characters.next()?;
    if first != '\\' {
        let (offset, character) = characters.next()?;
        return (character == '\'').then_some(offset + 2);
    }
    characters.next()?;
    let (offset, _) = characters.find(|(_, character)| *character == '\'')?;
    Some(offset + 2)
}
