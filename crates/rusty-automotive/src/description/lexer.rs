use std::io::BufRead;
use std::sync::atomic::Ordering;

use super::DescriptionError;
use crate::Watch;

const CHUNK: usize = 64 * 1024;
const MAX_TOKEN: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word,
    Quoted,
    EndLine,
    Semicolon,
    Problem,
}

pub(crate) struct Token {
    pub text: String,
    pub line: u64,
    pub kind: TokenKind,
    pub indented: bool,
}

pub(crate) struct Lexer<'a, R> {
    source: R,
    watch: Watch<'a>,
    buffer: Vec<u8>,
    at: usize,
    filled: usize,
    offset: u64,
    line: u64,
    fresh: bool,
    indent: bool,
}

impl<'a, R: BufRead> Lexer<'a, R> {
    pub(crate) fn new(source: R, watch: Watch<'a>) -> Self {
        Self {
            source,
            watch,
            buffer: vec![0; CHUNK],
            at: 0,
            filled: 0,
            offset: 0,
            line: 1,
            fresh: true,
            indent: false,
        }
    }

    pub(crate) fn position(&self) -> (u64, u64) {
        (self.offset, self.line)
    }

    pub(crate) fn finish(&mut self) -> Result<(), DescriptionError> {
        self.check()?;
        (self.watch.progress)(self.offset);
        self.check()
    }

    fn check(&self) -> Result<(), DescriptionError> {
        if self.watch.cancel.load(Ordering::Relaxed) {
            Err(DescriptionError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn peek(&mut self) -> Result<Option<u8>, DescriptionError> {
        if self.at == self.filled {
            self.check()?;
            (self.watch.progress)(self.offset);
            self.check()?;
            self.filled = self.source.read(&mut self.buffer)?;
            self.at = 0;
        }
        Ok((self.at < self.filled).then(|| self.buffer[self.at]))
    }

    fn take(&mut self) -> Result<Option<u8>, DescriptionError> {
        let byte = self.peek()?;
        if let Some(byte) = byte {
            self.at += 1;
            self.offset += 1;
            self.line += u64::from(byte == b'\n');
        }
        Ok(byte)
    }

    pub(crate) fn next(&mut self) -> Result<Option<Token>, DescriptionError> {
        while let Some(byte) = self.peek()? {
            let line = self.line;
            self.take()?;
            let indented = self.lead(byte);
            if let Some(mut token) = self.classify(byte, line)? {
                token.indented = indented;
                return Ok(Some(token));
            }
        }
        Ok(None)
    }

    fn lead(&mut self, byte: u8) -> bool {
        if byte == b'\n' {
            self.fresh = true;
            self.indent = false;
        } else if byte.is_ascii_whitespace() {
            self.indent |= self.fresh;
        } else {
            let indented = self.fresh && self.indent;
            self.fresh = false;
            return indented;
        }
        false
    }

    fn classify(&mut self, byte: u8, line: u64) -> Result<Option<Token>, DescriptionError> {
        let kind = match byte {
            b'\n' => TokenKind::EndLine,
            b';' => TokenKind::Semicolon,
            byte if byte.is_ascii_whitespace() => return Ok(None),
            b'"' => return self.quoted(line).map(Some),
            b'/' if matches!(self.peek()?, Some(b'/' | b'*')) => return self.comment(line),
            _ => return self.word(byte, line).map(Some),
        };
        Ok(Some(Token {
            text: char::from(byte).to_string(),
            line,
            kind,
            indented: false,
        }))
    }

    pub(crate) fn significant(&mut self) -> Result<Option<Token>, DescriptionError> {
        loop {
            let token = self.next()?;
            if token
                .as_ref()
                .is_none_or(|token| token.kind != TokenKind::EndLine)
            {
                return Ok(token);
            }
        }
    }

    fn word(&mut self, first: u8, line: u64) -> Result<Token, DescriptionError> {
        let mut bytes = vec![first];
        let mut too_long = false;
        while let Some(byte) = self.peek()? {
            if byte.is_ascii_whitespace() || matches!(byte, b'"' | b';') {
                break;
            }
            self.take()?;
            too_long |= bytes.len() >= MAX_TOKEN;
            if !too_long {
                bytes.push(byte);
            }
        }
        Ok(Self::token(bytes, line, TokenKind::Word, too_long))
    }

    fn quoted(&mut self, line: u64) -> Result<Token, DescriptionError> {
        let mut bytes = vec![b'"'];
        let mut escaped = false;
        let mut too_long = false;
        while let Some(byte) = self.take()? {
            too_long |= bytes.len() >= MAX_TOKEN;
            if !too_long {
                bytes.push(byte);
            }
            let closes = byte == b'"' && !escaped;
            if closes && self.peek()? != Some(b'"') {
                return Ok(Self::token(bytes, line, TokenKind::Quoted, too_long));
            }
            if closes {
                self.take()?;
                bytes.extend((!too_long).then_some(b'"'));
            }
            escaped = byte == b'\\' && !escaped;
        }
        Ok(Token {
            text: "unterminated quoted string".into(),
            line,
            kind: TokenKind::Problem,
            indented: false,
        })
    }

    fn comment(&mut self, line: u64) -> Result<Option<Token>, DescriptionError> {
        let block = self.take()? == Some(b'*');
        let mut previous = 0;
        while let Some(byte) = self.peek()? {
            if !block && byte == b'\n' {
                return Ok(None);
            }
            self.take()?;
            if block && previous == b'*' && byte == b'/' {
                return Ok(None);
            }
            previous = byte;
        }
        Ok(block.then(|| Token {
            text: "unterminated block comment".into(),
            line,
            kind: TokenKind::Problem,
            indented: false,
        }))
    }

    fn token(bytes: Vec<u8>, line: u64, kind: TokenKind, too_long: bool) -> Token {
        if too_long {
            return Token {
                text: "token exceeds 64 KiB".into(),
                line,
                kind: TokenKind::Problem,
                indented: false,
            };
        }
        let mut text = String::from_utf8(bytes).unwrap_or_else(|error| {
            let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(error.as_bytes());
            text.into_owned()
        });
        if line == 1 && text.starts_with('\u{feff}') {
            text.drain(..3);
        }
        Token {
            text,
            line,
            kind,
            indented: false,
        }
    }
}
