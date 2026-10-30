#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Ident(String),
    Punct(char),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spanned {
    pub token: Token,
    pub line: usize,
}

pub(crate) fn tokenize(source: &str) -> Vec<Spanned> {
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        pos: 0,
        line: 1,
        out: Vec::new(),
    };
    while lexer.pos < lexer.chars.len() {
        lexer.step();
    }
    lexer.out
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    out: Vec<Spanned>,
}

impl Lexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn push(&mut self, token: Token, line: usize) {
        self.out.push(Spanned { token, line });
    }

    fn step(&mut self) {
        let Some(c) = self.peek(0) else { return };
        let line = self.line;
        match c {
            '/' if self.peek(1) == Some('/') => self.skip_line(),
            '/' if self.peek(1) == Some('*') => self.skip_block(),
            '"' => {
                let text = self.string();
                self.push(Token::Str(text), line);
            }
            '\'' => self.quote(),
            c if c.is_alphanumeric() || c == '_' => self.word(),
            c if c.is_whitespace() => {
                self.bump();
            }
            c => {
                self.bump();
                self.push(Token::Punct(c), line);
            }
        }
    }

    fn skip_line(&mut self) {
        while self.peek(0).is_some_and(|c| c != '\n') {
            self.bump();
        }
    }

    fn skip_block(&mut self) {
        let mut depth = 0usize;
        while let Some(c) = self.bump() {
            depth = match (c, self.peek(0)) {
                ('/', Some('*')) => {
                    self.bump();
                    depth + 1
                }
                ('*', Some('/')) => {
                    self.bump();
                    depth - 1
                }
                _ => depth,
            };
            if depth == 0 {
                return;
            }
        }
    }

    fn string(&mut self) -> String {
        self.bump();
        let mut text = String::new();
        while let Some(c) = self.bump() {
            match c {
                '"' => break,
                '\\' => self.escape(&mut text),
                c => text.push(c),
            }
        }
        text
    }

    fn escape(&mut self, text: &mut String) {
        match self.bump() {
            Some('n' | 't' | 'r') => text.push(' '),
            Some('\n') => {
                while self.peek(0).is_some_and(char::is_whitespace) {
                    self.bump();
                }
            }
            Some('u') => {
                while self.bump().is_some_and(|c| c != '}') {}
                text.push('?');
            }
            Some('x') => {
                self.bump();
                self.bump();
                text.push('?');
            }
            Some(other) => text.push(other),
            None => {}
        }
    }

    fn raw_string(&mut self) -> Option<String> {
        let mut hashes = 0;
        while self.peek(0) == Some('#') {
            self.bump();
            hashes += 1;
        }
        if self.peek(0) != Some('"') {
            return None;
        }
        self.bump();
        let mut text = String::new();
        while let Some(c) = self.bump() {
            if c == '"' && (0..hashes).all(|ahead| self.peek(ahead) == Some('#')) {
                self.pos += hashes;
                break;
            }
            text.push(c);
        }
        Some(text)
    }

    fn quote(&mut self) {
        let is_char = matches!(
            (self.peek(1), self.peek(2)),
            (Some('\\'), _) | (Some(_), Some('\''))
        );
        self.bump();
        if !is_char {
            return;
        }
        while let Some(c) = self.bump() {
            match c {
                '\\' => {
                    self.bump();
                }
                '\'' => break,
                _ => {}
            }
        }
    }

    fn word(&mut self) {
        let line = self.line;
        let mut word = String::new();
        while let Some(c) = self.peek(0).filter(|c| c.is_alphanumeric() || *c == '_') {
            word.push(c);
            self.bump();
        }
        match (word.as_str(), self.peek(0)) {
            ("r" | "br" | "cr", Some('"' | '#')) => match self.raw_string() {
                Some(text) if word == "r" => self.push(Token::Str(text), line),
                Some(_) => {}
                None => self.push(Token::Ident(word), line),
            },
            ("b" | "c", Some('"')) => {
                self.string();
            }
            _ => self.push(Token::Ident(word), line),
        }
    }
}
