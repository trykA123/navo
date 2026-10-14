#[derive(Default)]
enum State {
    #[default]
    Code,
    BlockComment(usize),
    String,
    RawString(usize),
}

pub fn count_code_lines(source: &str) -> usize {
    let mut state = State::default();
    source.lines().filter(|line| state.count_line(line)).count()
}

impl State {
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
        match *self {
            Self::Code => self.consume_code(input),
            Self::BlockComment(depth) => (self.consume_comment(input, depth), false),
            Self::String => (self.consume_string(input), true),
            Self::RawString(hashes) => (self.consume_raw_string(input, hashes), true),
        }
    }

    fn consume_code(&mut self, input: &str) -> (usize, bool) {
        if input.starts_with("//") {
            return (input.len(), false);
        }
        if input.starts_with("/*") {
            *self = Self::BlockComment(1);
            return (2, false);
        }
        if let Some((length, hashes)) = raw_string_start(input.as_bytes()) {
            *self = Self::RawString(hashes);
            return (length, true);
        }
        if let Some(length) = char_literal_length(input) {
            return (length, true);
        }
        let character = input.chars().next();
        if character == Some('"') {
            *self = Self::String;
        }
        (
            character.map_or(0, char::len_utf8),
            character.is_some_and(|character| !character.is_whitespace()),
        )
    }

    fn consume_comment(&mut self, input: &str, depth: usize) -> usize {
        if input.starts_with("/*") {
            *self = Self::BlockComment(depth + 1);
            return 2;
        }
        if input.starts_with("*/") {
            *self = if depth == 1 {
                Self::Code
            } else {
                Self::BlockComment(depth - 1)
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
                *self = Self::Code;
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
            *self = Self::Code;
            return hashes + 1;
        }
        input.chars().next().map_or(0, char::len_utf8)
    }
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
