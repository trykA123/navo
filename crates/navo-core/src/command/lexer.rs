use super::{ParseError, ParseErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value<'a> {
    Str(String),
    Int(&'a str),
    Word(&'a str),
}

pub(crate) struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}

const MAX_HEX_DIGITS: usize = 6;

impl<'a> Lexer<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.text
            .get(self.pos..)
            .and_then(|rest| rest.chars().next())
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    pub(crate) fn skip_ws(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
        {
            self.pos += 1;
        }
    }

    pub(crate) fn error(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            offset: self.pos,
            kind,
        }
    }

    pub(crate) fn unexpected(&self) -> ParseError {
        self.error(self.peek().map_or(
            ParseErrorKind::UnexpectedEnd,
            ParseErrorKind::UnexpectedChar,
        ))
    }

    pub(crate) fn eat(&mut self, expected: char) -> Result<(), ParseError> {
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    pub(crate) fn word(&mut self) -> Result<&'a str, ParseError> {
        let start = self.pos;
        if !self.peek().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err(self.unexpected());
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            self.pos += 1;
        }
        Ok(&self.text[start..self.pos])
    }

    pub(crate) fn value(&mut self) -> Result<Value<'a>, ParseError> {
        match self.peek() {
            Some('"') => self.string().map(Value::Str),
            Some(c) if c == '-' || c.is_ascii_digit() => self.integer().map(Value::Int),
            Some(c) if c.is_ascii_lowercase() => self.word().map(Value::Word),
            _ => Err(self.unexpected()),
        }
    }

    fn integer(&mut self) -> Result<&'a str, ParseError> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.unexpected());
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        Ok(&self.text[start..self.pos])
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let open = self.pos;
        let unterminated = ParseError {
            offset: open,
            kind: ParseErrorKind::UnterminatedString,
        };
        self.pos += 1;
        let mut out = String::new();
        loop {
            let at = self.pos;
            match self.bump().ok_or_else(|| unterminated.clone())? {
                '"' => return Ok(out),
                '\\' => out.push(self.escape(at).map_err(|e| e.or(&unterminated))?),
                c => out.push(c),
            }
        }
    }

    fn escape(&mut self, backslash: usize) -> Result<char, EscapeError> {
        let bad = EscapeError::Bad(backslash);
        match self.bump().ok_or(EscapeError::End)? {
            '"' => Ok('"'),
            '\\' => Ok('\\'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            'u' => self.unicode().map_err(|e| e.at(backslash)),
            _ => Err(bad),
        }
    }

    fn unicode(&mut self) -> Result<char, EscapeError> {
        if self.bump().ok_or(EscapeError::End)? != '{' {
            return Err(EscapeError::Bad(0));
        }
        let mut code: u32 = 0;
        let mut digits = 0;
        loop {
            let c = self.bump().ok_or(EscapeError::End)?;
            if c == '}' {
                break;
            }
            let digit = c.to_digit(16).ok_or(EscapeError::Bad(0))?;
            digits += 1;
            if digits > MAX_HEX_DIGITS {
                return Err(EscapeError::Bad(0));
            }
            code = code * 16 + digit;
        }
        if digits == 0 {
            return Err(EscapeError::Bad(0));
        }
        char::from_u32(code).ok_or(EscapeError::Bad(0))
    }
}

enum EscapeError {
    Bad(usize),
    End,
}

impl EscapeError {
    fn at(self, backslash: usize) -> Self {
        match self {
            Self::Bad(_) => Self::Bad(backslash),
            Self::End => Self::End,
        }
    }

    fn or(self, unterminated: &ParseError) -> ParseError {
        match self {
            Self::Bad(offset) => ParseError {
                offset,
                kind: ParseErrorKind::BadEscape,
            },
            Self::End => unterminated.clone(),
        }
    }
}
