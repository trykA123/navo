//! Classic find form: one field per token, a canonical token line and token spans for the live
//! token bar. Each field is printed and validated through the `FindQuery` token grammar, so the
//! form never invents a spelling the query parser cannot read back.

use std::ops::Range;

use navo_core::find::{FindQuery, GitRules, TextEncoding, TokenKind};

/// One editable field of the classic find form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicField {
    Text,
    Case,
    Regex,
    Word,
    Not,
    Hex,
    Encodings,
    Names,
    Folders,
    Depth,
    Archives,
    GitOff,
    Docs,
    Ext,
    Size,
    Modified,
    Attrs,
}

/// Raw values of every classic form field, before they become a [`FindQuery`].
///
/// Free-text filters hold the token value only (`"<1048576"`, not `"size:<1048576"`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassicForm {
    pub text: String,
    pub case: bool,
    pub regex: bool,
    pub word: bool,
    pub not: bool,
    pub hex: String,
    pub encodings: Vec<TextEncoding>,
    pub names: String,
    pub folders: Vec<String>,
    pub depth: Option<u32>,
    pub archives: bool,
    pub git_off: bool,
    pub docs: bool,
    pub ext: String,
    pub size: String,
    pub modified: String,
    pub attrs: String,
}

/// The marker that turns the text token into a regex.
const REGEX_MARK: &str = "re:";

/// One printed token and the field that owns it.
struct Chunk {
    field: ClassicField,
    text: String,
}

impl Chunk {
    fn new(field: ClassicField, text: impl Into<String>) -> Self {
        Self {
            field,
            text: text.into(),
        }
    }
}

impl ClassicForm {
    /// Builds the query, reporting every field whose token does not parse.
    ///
    /// # Errors
    ///
    /// Returns one `(field, message)` entry per rejected field, in token order.
    pub fn to_query(&self) -> Result<FindQuery, Vec<(ClassicField, String)>> {
        let query = FindQuery::parse(&self.to_text());
        let mut errors = query
            .tokens
            .iter()
            .filter_map(|token| {
                let message = token.message.as_ref()?;
                Some((field_of(token.kind), message.to_string()))
            })
            .collect::<Vec<_>>();
        errors.extend(combo_errors(&query));
        if errors.is_empty() {
            Ok(query)
        } else {
            Err(errors)
        }
    }

    /// Prints the canonical token line, keeping a field's text raw while it is mid-edit.
    #[must_use]
    pub fn to_text(&self) -> String {
        let chunks = self.chunks();
        let parts = chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>();
        parts.join(" ")
    }

    /// Fills every field from a query; the inverse of [`ClassicForm::to_query`].
    #[must_use]
    pub fn from_query(query: &FindQuery) -> Self {
        Self {
            text: query.text.clone(),
            case: query.case,
            regex: query.regex,
            word: query.word,
            not: query.invert,
            hex: query.hex.as_deref().map(hex_text).unwrap_or_default(),
            encodings: query.encodings.clone(),
            names: query.name.clone().unwrap_or_default(),
            folders: query.within.clone(),
            depth: query.depth,
            archives: query.archives,
            git_off: query.git_rules == GitRules::Off,
            docs: query.docs,
            ext: query.ext.join(","),
            size: rendered("size:", |built| built.size = query.size),
            modified: rendered("mod:", |built| built.modified = query.modified),
            attrs: rendered("attr:", |built| built.attrs.clone_from(&query.attrs)),
        }
    }

    /// Byte range of `field`'s token inside [`ClassicForm::to_text`].
    ///
    /// A field that prints several tokens (`folders`) reports the range covering all of them, and
    /// [`ClassicField::Regex`] reports the `re:` marker of the text token. Fields that print
    /// nothing (an `off` toggle, an empty filter) have no span.
    #[must_use]
    pub fn token_span(&self, field: ClassicField) -> Option<Range<usize>> {
        let chunks = self.chunks();
        let mut offset = 0;
        let mut spans = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let start = offset;
            offset += chunk.text.len() + 1;
            spans.push((chunk.field, start..offset - 1));
        }
        if field == ClassicField::Regex {
            let (_, text) = spans
                .iter()
                .find(|(owner, _)| *owner == ClassicField::Text)?;
            return self
                .regex
                .then(|| text.start..text.start + REGEX_MARK.len());
        }
        spans
            .iter()
            .filter(|(owner, _)| *owner == field)
            .map(|(_, span)| span.clone())
            .reduce(|first, next| first.start..next.end)
    }

    /// The tokens of this form in the canonical print order of [`FindQuery::to_text`].
    fn chunks(&self) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        if !self.text.is_empty() {
            let text = text_token(&self.text, self.regex);
            chunks.push(Chunk::new(ClassicField::Text, text));
        }
        push_filter(&mut chunks, ClassicField::Ext, "ext:", &self.ext);
        push_filter(&mut chunks, ClassicField::Size, "size:", &self.size);
        push_filter(&mut chunks, ClassicField::Modified, "mod:", &self.modified);
        push_filter(&mut chunks, ClassicField::Attrs, "attr:", &self.attrs);
        for folder in &self.folders {
            chunks.push(Chunk::new(ClassicField::Folders, folder_token(folder)));
        }
        push_flag(&mut chunks, ClassicField::Archives, self.archives, "arc:on");
        push_flag(&mut chunks, ClassicField::GitOff, self.git_off, "git:off");
        if !self.names.is_empty() {
            chunks.push(Chunk::new(ClassicField::Names, names_token(&self.names)));
        }
        if let Some(depth) = self.depth {
            chunks.push(Chunk::new(ClassicField::Depth, format!("depth:{depth}")));
        }
        push_flag(&mut chunks, ClassicField::Word, self.word, "word:on");
        push_flag(&mut chunks, ClassicField::Not, self.not, "not:on");
        push_flag(&mut chunks, ClassicField::Case, self.case, "case:on");
        push_filter(&mut chunks, ClassicField::Hex, "hex:", &self.hex);
        if !self.encodings.is_empty() {
            let token = encodings_token(&self.encodings);
            chunks.push(Chunk::new(ClassicField::Encodings, token));
        }
        push_flag(&mut chunks, ClassicField::Docs, self.docs, "docs:on");
        chunks
    }
}

/// Adds `prefix` + `value` as a token when the field is filled.
fn push_filter(chunks: &mut Vec<Chunk>, field: ClassicField, prefix: &str, value: &str) {
    if !value.is_empty() {
        chunks.push(Chunk::new(field, filter_token(prefix, value)));
    }
}

/// Adds a fixed token when the flag is on.
fn push_flag(chunks: &mut Vec<Chunk>, field: ClassicField, on: bool, token: &str) {
    if on {
        chunks.push(Chunk::new(field, token));
    }
}

/// Renders one typed filter token canonically, or as typed while the value is mid-edit.
fn filter_token(prefix: &str, value: &str) -> String {
    let raw = format!("{prefix}{}", raw_value(value));
    let query = FindQuery::parse(&raw);
    if query.tokens.len() == 1 && query.tokens[0].message.is_none() {
        query.to_text()
    } else {
        raw
    }
}

/// Prints the search text, as one `re:` pattern when regex is on.
fn text_token(text: &str, regex: bool) -> String {
    let query = FindQuery {
        text: text.to_owned(),
        regex,
        ..FindQuery::default()
    };
    query.to_text()
}

/// Prints one `name:` value through the query printer, so quoting is never re-invented.
fn names_token(value: &str) -> String {
    let query = FindQuery {
        name: Some(value.to_owned()),
        ..FindQuery::default()
    };
    query.to_text()
}

/// Prints one `in:` folder through the query printer, so quoting is never re-invented.
fn folder_token(folder: &str) -> String {
    let query = FindQuery {
        within: vec![folder.to_owned()],
        ..FindQuery::default()
    };
    query.to_text()
}

/// Prints the chosen encodings as one `enc:` list.
fn encodings_token(encodings: &[TextEncoding]) -> String {
    let query = FindQuery {
        encodings: encodings.to_vec(),
        ..FindQuery::default()
    };
    query.to_text()
}

/// Renders one typed filter back to its value, dropping the token key.
fn rendered(prefix: &str, fill: impl FnOnce(&mut FindQuery)) -> String {
    let mut query = FindQuery::default();
    fill(&mut query);
    let text = query.to_text();
    text.strip_prefix(prefix).unwrap_or(&text).to_owned()
}

/// Prints bytes as the lowercase hex pairs a `hex:` token reads back.
fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Quotes a value so the parser reads it back as one token while it is mid-edit.
///
/// Mirrors the query printer: a literal quote followed by whitespace closes and reopens the
/// quote, so `a" b` never splits into two tokens or leaks its error onto another field.
fn raw_value(value: &str) -> String {
    let needs_quotes =
        value.contains(char::is_whitespace) || value.contains(':') || value.contains('"');
    if !needs_quotes {
        return value.to_owned();
    }
    let mut out = String::from("\"");
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            out.push(ch);
            continue;
        }
        if chars.peek().is_some_and(|next| next.is_whitespace()) {
            out.push_str("\\\"\"\"");
        } else {
            out.push_str("\\\"");
        }
    }
    out.push('"');
    out
}

/// Matcher-only token combinations, reported against the field that breaks them.
///
/// Mirrors `validate_query` in the core matcher; `word:on` with `re:` is a valid pair.
fn combo_errors(query: &FindQuery) -> Vec<(ClassicField, String)> {
    let mut errors = Vec::new();
    if query.hex.is_some() && (query.regex || query.word || !query.text.is_empty()) {
        errors.push((
            ClassicField::Hex,
            "hex: cannot be combined with re:, word: or text".to_owned(),
        ));
    }
    if query.invert && (query.text.is_empty() || query.hex.is_some()) {
        errors.push((ClassicField::Not, "not: needs text to look for".to_owned()));
    }
    errors
}

/// The form field a parsed token kind belongs to.
fn field_of(kind: TokenKind) -> ClassicField {
    match kind {
        TokenKind::Text | TokenKind::Regex => ClassicField::Text,
        TokenKind::Ext => ClassicField::Ext,
        TokenKind::Size => ClassicField::Size,
        TokenKind::Modified => ClassicField::Modified,
        TokenKind::Attrs => ClassicField::Attrs,
        TokenKind::Within => ClassicField::Folders,
        TokenKind::Archives => ClassicField::Archives,
        TokenKind::Git => ClassicField::GitOff,
        TokenKind::Name => ClassicField::Names,
        TokenKind::Depth => ClassicField::Depth,
        TokenKind::Word => ClassicField::Word,
        TokenKind::Not => ClassicField::Not,
        TokenKind::Case => ClassicField::Case,
        TokenKind::Hex => ClassicField::Hex,
        TokenKind::Enc => ClassicField::Encodings,
        TokenKind::Docs => ClassicField::Docs,
    }
}
