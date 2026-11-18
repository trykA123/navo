//! The exact split checker for the rule lexer (D7): a line lexed in chunks must colour every byte
//! like the whole line, except inside a token that a chunk end physically cut.

use std::ops::Range;

use navo_syntax::Language;
use sidewise_kit::languages::lines::{LineLexer, LineState, Span};

use super::RuleLexer;

/// The role of a byte no span covers.
const PLAIN: &str = "plain";

/// One test line, lexed whole as the editor does, ready to be checked against chunked lexing.
pub(super) struct Line<'a> {
    lexer: &'a RuleLexer,
    language: &'a Language,
    text: &'a str,
    state: LineState,
    whole: Vec<Span>,
    /// The state the whole line leaves for the next line.
    whole_end: LineState,
}

impl<'a> Line<'a> {
    /// Lexes `text` whole from `state`.
    pub(super) fn new(
        lexer: &'a RuleLexer,
        language: &'a Language,
        text: &'a str,
        state: LineState,
    ) -> Self {
        let (whole, whole_end) = lexer.lex_last(text, state);
        Self {
            lexer,
            language,
            text,
            state,
            whole,
            whole_end,
        }
    }

    /// Lexes the line in chunks cut at `cuts`, as the paged view does; returns the spans and the
    /// state the last chunk leaves.
    pub(super) fn chunked(&self, cuts: &[usize]) -> (Vec<Span>, LineState) {
        let (mut chunked, mut carried, mut start) = (Vec::new(), self.state, 0);
        for end in cuts.iter().copied().chain(std::iter::once(self.text.len())) {
            let chunk = &self.text[start..end];
            let (spans, next) = if end == self.text.len() {
                self.lexer.lex_last(chunk, carried)
            } else {
                self.lexer.lex(chunk, carried)
            };
            chunked.extend(
                spans
                    .into_iter()
                    .map(|(range, role)| (start + range.start..start + range.end, role)),
            );
            carried = next;
            start = end;
        }
        (chunked, carried)
    }

    /// The whole-line span that holds `cut` strictly inside it, with its opener length when it
    /// is a comment or string (`None` for any other token).
    fn cut_token(&self, cut: usize) -> Option<(&Span, Option<usize>)> {
        let token = self
            .whole
            .iter()
            .find(|(range, _)| range.start < cut && cut < range.end)?;
        let from_start = self.state.0 != 0 && token.0.start == 0;
        let text = self.text.get(token.0.clone())?;
        Some((token, opener_len(self.language, text, token.1, from_start)))
    }

    /// The byte ranges whose colour a cut may change (D7): only the bytes of the whole-line
    /// token that holds the cut. The flag is true for a plain word, which frees only its
    /// bytes that the whole line left plain.
    ///
    /// A cut inside a comment or string after its opener, closer included, changes nothing: the
    /// carried state is exact. A cut inside an opener may change the opener's bytes; a cut inside
    /// any other token may change that token only. A cut exactly on a token's edge changes
    /// nothing, even when a word runs across it.
    fn exempt_ranges(&self, cuts: &[usize]) -> Vec<(Range<usize>, bool)> {
        let mut exempt = Vec::new();
        for &cut in cuts {
            match self.cut_token(cut) {
                Some(((range, _), Some(open))) if cut >= range.start + open => {}
                Some(((range, _), Some(open))) => {
                    exempt.push((range.start..range.start + open, false));
                }
                Some(((range, _), None)) => exempt.push((range.clone(), false)),
                None if self.on_token_edge(cut) => {}
                None => exempt.extend(self.plain_word(cut).map(|word| (word, true))),
            }
        }
        exempt
    }

    /// True when the cut falls exactly where a whole-line token starts or ends: no token holds
    /// it, so no byte may change colour.
    fn on_token_edge(&self, cut: usize) -> bool {
        self.whole
            .iter()
            .any(|(range, _)| range.start == cut || range.end == cut)
    }

    /// The word run that holds the cut, when a word character lies on both sides of it.
    fn plain_word(&self, cut: usize) -> Option<Range<usize>> {
        let (head, tail) = (&self.text[..cut], &self.text[cut..]);
        if !head.chars().next_back().is_some_and(is_word) || !tail.starts_with(is_word) {
            return None;
        }
        let start = head
            .char_indices()
            .rev()
            .take_while(|(_, ch)| is_word(*ch))
            .last()
            .map_or(cut, |(at, _)| at);
        Some(start..cut + tail.find(|ch: char| !is_word(ch)).unwrap_or(tail.len()))
    }

    /// Checks one split: every byte outside the exempt ranges has the same role whole and chunked,
    /// and the last chunk leaves the state the whole line leaves.
    pub(super) fn check(&self, cuts: &[usize]) -> Result<(), String> {
        let (chunked, end) = self.chunked(cuts);
        let exempt = self.exempt_ranges(cuts);
        let (a, b) = (roles(self.text, &self.whole), roles(self.text, &chunked));
        for (at, (whole_role, chunk_role)) in a.iter().zip(&b).enumerate() {
            let free = exempt
                .iter()
                .any(|(range, plain)| range.contains(&at) && (!plain || *whole_role == PLAIN));
            if whole_role != chunk_role && !free {
                return Err(format!(
                    "byte {at}: whole {whole_role}, chunked {chunk_role}; whole {:?}; chunked {chunked:?}",
                    self.whole
                ));
            }
        }
        if end != self.whole_end {
            return Err(format!(
                "the line ends in {end:?} chunked, {:?} whole",
                self.whole_end
            ));
        }
        Ok(())
    }
}

/// Gives every byte of `line` its span's role, or `plain`.
fn roles(line: &str, spans: &[Span]) -> Vec<&'static str> {
    let mut roles = vec![PLAIN; line.len()];
    for (range, role) in spans {
        roles[range.clone()].fill(role);
    }
    roles
}

/// True for a character that can be part of a word.
fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// How a comment or string token starts: the opener's length, or None for another token.
fn opener_len(language: &Language, text: &str, role: &str, from_start: bool) -> Option<usize> {
    if from_start {
        return Some(0);
    }
    let comments = language
        .line_comments
        .iter()
        .chain(language.block_comments.iter().map(|(open, _)| open))
        .filter(|_| role == "comment");
    let strings = language
        .delimiters
        .iter()
        .filter(|d| d.kind.highlight_name() == role)
        .map(|d| &d.open);
    comments
        .chain(strings)
        .filter(|open| !open.is_empty() && text.starts_with(open.as_str()))
        .map(String::len)
        .max()
}
