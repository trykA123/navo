#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Mixed,
    Commands,
    Files,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Name(String),
    Path(String),
    Ext(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub scope: Scope,
    pub terms: Vec<Term>,
    pub path: Option<String>,
}

const EXT_KEY: &str = "ext:";

impl Query {
    #[must_use]
    pub fn parse(input: &str) -> Self {
        if is_path_mode(input) {
            return Self {
                scope: Scope::Files,
                terms: Vec::new(),
                path: Some(input.to_owned()),
            };
        }
        let (scope, rest) = match input.chars().next() {
            Some('>') => (Scope::Commands, &input[1..]),
            Some('/') => (Scope::Files, &input[1..]),
            _ => (Scope::Mixed, input),
        };
        Self {
            scope,
            terms: rest.split_whitespace().filter_map(parse_term).collect(),
            path: None,
        }
    }

    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.terms.is_empty() && self.path.is_none()
    }
}

fn is_path_mode(input: &str) -> bool {
    let mut chars = input.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) => letter.is_ascii_alphabetic(),
        (Some('\\'), Some('\\')) => true,
        (Some('~'), None | Some('/' | '\\')) => true,
        _ => false,
    }
}

fn parse_term(word: &str) -> Option<Term> {
    match word.get(..EXT_KEY.len()) {
        Some(key) if key.eq_ignore_ascii_case(EXT_KEY) => parse_ext(&word[EXT_KEY.len()..]),
        _ if word.contains(['\\', '/']) => Some(Term::Path(word.to_owned())),
        _ => Some(Term::Name(word.to_owned())),
    }
}

fn parse_ext(list: &str) -> Option<Term> {
    let exts: Vec<String> = list
        .split(',')
        .map(|ext| ext.trim_start_matches('.').to_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect();
    (!exts.is_empty()).then_some(Term::Ext(exts))
}
