//! Streaming line lexer contract shared by core consumers.

mod json;
mod log;
mod log_shapes;
mod log_words;
mod markup;

use std::ops::Range;
pub type Span = (Range<usize>, &'static str);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LineState(pub u32);
impl LineState {
    /// Returns the carried opening prefix length in bytes.
    pub fn prefix_len(self) -> usize {
        ((self.0 >> 8) & 0xF) as usize
    }
}
pub const MAX_LINE: usize = 64 * 1024;
pub const MAX_SPANS: usize = 4096;
pub const MAX_PAGE_BYTES: usize = 1024 * 1024;
pub const MAX_PAGE_SPANS: usize = 65_536;
pub const ROLES: [&str; 20] = [
    "title",
    "keyword",
    "function",
    "comment",
    "comment.doc",
    "attribute",
    "number",
    "string",
    "string.special",
    "link_uri",
    "constructor",
    "tag",
    "punctuation.delimiter",
    "punctuation.bracket",
    "property",
    "boolean",
    "constant",
    "type",
    "operator",
    "variable.special",
];
pub trait LineLexer: Send + Sync {
    /// Returns the short language name.
    fn name(&self) -> &str;
    /// Tells two lexers with one name apart; 0 (default) means the name alone decides.
    fn identity(&self) -> usize {
        0
    }
    /// Returns chunk-relative spans and the carried state.
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState);
    /// Lexes a chunk that ends its line, so a token reaching the chunk end is complete.
    /// Callers still apply `line_end` afterwards. Default: same as `lex`.
    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.lex(chunk, state)
    }
    /// True when every line starts from the default state, so a caller may lex any line alone.
    fn line_local(&self) -> bool {
        false
    }
    /// Clears carried delimiters after a line break.
    fn line_end(&self, state: LineState) -> LineState {
        LineState(state.0 & !0xFF00)
    }
    /// Guesses a start state from a page head, or returns None for plain text.
    fn resync(&self, _head: &str) -> Option<LineState> {
        Some(LineState::default())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineLanguage {
    Log,
    Xml,
    Json,
}
impl LineLanguage {
    /// Returns the language for a supported exact name.
    pub fn for_language(name: &str) -> Option<Self> {
        match name {
            "xml" => Some(Self::Xml),
            "json" | "jsonc" | "json5" => Some(Self::Json),
            "log" | "text" => Some(Self::Log),
            _ => None,
        }
    }
}
impl LineLexer for LineLanguage {
    fn name(&self) -> &str {
        match self {
            Self::Log => "log",
            Self::Xml => "xml",
            Self::Json => "json",
        }
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        match self {
            Self::Log => log::lex(chunk, state, false),
            Self::Xml => markup::lex(chunk, state),
            Self::Json => json::lex(chunk, state),
        }
    }
    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        match self {
            Self::Log => log::lex(chunk, state, true),
            _ => self.lex(chunk, state),
        }
    }
    fn line_local(&self) -> bool {
        matches!(self, Self::Log)
    }
    fn resync(&self, head: &str) -> Option<LineState> {
        match self {
            Self::Xml => markup::resync(head),
            Self::Json => json::resync(head),
            _ => Some(LineState::default()),
        }
    }
    fn line_end(&self, state: LineState) -> LineState {
        match self {
            Self::Log => log::line_end(state),
            Self::Xml => markup::line_end(state),
            Self::Json => json::line_end(state),
        }
    }
}
/// Checks chunk spans and returns the first violation.
pub fn check_spans(chunk: &str, spans: &[Span]) -> Result<(), String> {
    if spans.len() > MAX_SPANS {
        return Err(format!("span {MAX_SPANS}: exceeds MAX_SPANS"));
    }
    let mut previous: Option<&Range<usize>> = None;
    for (index, (range, role)) in spans.iter().enumerate() {
        let reason = if range.start >= range.end {
            Some("empty")
        } else if range.end > chunk.len() {
            Some("out of chunk")
        } else if !chunk.is_char_boundary(range.start) || !chunk.is_char_boundary(range.end) {
            Some("not on char boundary")
        } else if !ROLES.contains(role) {
            Some("unknown role")
        } else if previous.is_some_and(|p| range.start < p.start) {
            Some("unordered")
        } else if previous.is_some_and(|p| range.start < p.end) {
            Some("overlapping")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(format!("span {index}: {reason}"));
        }
        previous = Some(range);
    }
    Ok(())
}
/// Checks that cuts rise strictly and sit on character boundaries inside the line.
fn check_cuts(line: &str, cuts: &[usize]) -> Result<(), String> {
    let mut previous = None;
    for &cut in cuts {
        if previous.is_some_and(|p| cut <= p) || cut > line.len() || !line.is_char_boundary(cut) {
            return Err(format!("invalid cut at byte {cut}"));
        }
        previous = Some(cut);
    }
    Ok(())
}

/// Checks split safety, returning a description of the first mismatch.
/// Every chunk, and the whole line, is lexed with `lex`.
#[expect(
    clippy::too_many_arguments,
    reason = "The streaming contract fixes this public signature."
)]
pub fn check_split(
    lexer: &dyn LineLexer,
    line: &str,
    cuts: &[usize],
    state: LineState,
    accept: &[(&str, &str)],
) -> Result<(), String> {
    check_split_mode(lexer, line, cuts, state, accept, false)
}

/// Checks split safety like production: the whole line and the final chunk are
/// lexed with `lex_last` (they end the line), the earlier chunks with `lex`.
#[expect(
    clippy::too_many_arguments,
    reason = "The streaming contract fixes this public signature."
)]
pub fn check_split_line_end(
    lexer: &dyn LineLexer,
    line: &str,
    cuts: &[usize],
    state: LineState,
    accept: &[(&str, &str)],
) -> Result<(), String> {
    check_split_mode(lexer, line, cuts, state, accept, true)
}

/// Shared split check; `line_end` lexes the whole line and the last chunk with `lex_last`.
#[expect(
    clippy::too_many_arguments,
    reason = "It mirrors the public signature plus the mode flag."
)]
fn check_split_mode(
    lexer: &dyn LineLexer,
    line: &str,
    cuts: &[usize],
    state: LineState,
    accept: &[(&str, &str)],
    line_end: bool,
) -> Result<(), String> {
    check_cuts(line, cuts)?;
    let (whole, _) = if line_end {
        lexer.lex_last(line, state)
    } else {
        lexer.lex(line, state)
    };
    check_spans(line, &whole)?;
    let mut horizon = if whole.len() == MAX_SPANS {
        whole[MAX_SPANS - 1].0.end
    } else {
        line.len()
    };
    let mut exempt = vec![false; line.len()];
    let mut chunked = Vec::new();
    let mut start = 0;
    let mut carried = state;
    for (index, end) in cuts
        .iter()
        .copied()
        .chain(std::iter::once(line.len()))
        .enumerate()
    {
        let chunk = &line[start..end];
        let (spans, next) = if line_end && index == cuts.len() {
            lexer.lex_last(chunk, carried)
        } else {
            lexer.lex(chunk, carried)
        };
        check_spans(chunk, &spans)?;
        if spans.len() == MAX_SPANS {
            horizon = horizon.min(start + spans[MAX_SPANS - 1].0.end);
        }
        for (range, role) in spans {
            chunked.push((start + range.start..start + range.end, role));
        }
        if index < cuts.len() {
            let prefix = next.prefix_len();
            if prefix > end {
                return Err(format!("invalid prefix at byte {end}"));
            }
            exempt[end - prefix..end].fill(true);
        }
        carried = next;
        start = end;
    }
    compare_split(&whole, &chunked, (&exempt, horizon), accept)
}

/// Compares visible split spans and returns the first role mismatch.
fn compare_split(
    whole: &[Span],
    chunked: &[Span],
    limits: (&[bool], usize),
    accept: &[(&str, &str)],
) -> Result<(), String> {
    let (exempt, horizon) = limits;
    for (range, role) in chunked {
        let mut visible = range
            .clone()
            .take_while(|&i| i < horizon)
            .filter(|&i| !exempt[i]);
        let Some(first) = visible.next() else {
            continue;
        };
        let last = visible.last().unwrap_or(first);
        let matching = span_at(whole, first);
        let whole_role = matching.map(|(_, r)| *r).unwrap_or("plain");
        if !matching.is_some_and(|(r, w)| {
            r.contains(&last) && (*w == *role || accept.contains(&(*role, *w)))
        }) {
            return Err(format!("byte {first}: chunked {role}, whole {whole_role}"));
        }
    }
    for (range, role) in whole {
        if !matches!(*role, "comment" | "string") {
            continue;
        }
        for offset in range
            .clone()
            .take_while(|&i| i < horizon)
            .filter(|&i| !exempt[i])
        {
            let chunk_role = span_at(chunked, offset).map(|(_, r)| *r).unwrap_or("plain");
            if chunk_role != *role && !accept.contains(&(chunk_role, *role)) {
                return Err(format!("byte {offset}: chunked {chunk_role}, whole {role}"));
            }
        }
    }
    Ok(())
}

/// Finds the ordered span containing a byte, or returns None for plain text.
fn span_at(spans: &[Span], offset: usize) -> Option<&Span> {
    let index = spans.partition_point(|(range, _)| range.end <= offset);
    spans
        .get(index)
        .filter(|(range, _)| range.contains(&offset))
}

#[cfg(test)]
mod tests_contract;

#[cfg(test)]
mod tests_markup;

#[cfg(test)]
mod tests_json;

#[cfg(test)]
mod tests_log;

#[cfg(test)]
mod tests_log_split;

#[cfg(test)]
mod tests_log_shapes;
