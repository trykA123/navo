//! Chunked lexing: carries a comment, a word, or the bytes of a pattern that a chunk end cut,
//! across the chunk end. Part of the rule lexer; `Lexer::line` is the whole-line case.
//!
//! Hold-back: at the end of a chunk that may continue, the earliest position where a comment or
//! string pattern could still be completing is held. The next chunk starts there, in the lexer
//! state that holds there, with the held bytes in front of its own.

use super::Lexer;
use super::scan::{word_before, word_len};
use super::spans::{MAX_TOKENS, Run, Step};
use crate::{ChunkState, Inside, Token};

/// How a text is fed to the lexer: whether it ends the line, how many bytes to skip at its start
/// and how many bytes after those an earlier chunk returned already.
#[derive(Clone, Copy)]
pub(super) struct Feed {
    pub(super) last: bool,
    pub(super) lead: usize,
    pub(super) held: usize,
}

/// A byte that counts as a word byte, put in front of held bytes that follow a word.
const WORD_BYTE: &str = "_";

impl Lexer {
    /// Lexes one chunk of a line that starts in `state`; `last` is true when no later chunk
    /// continues the line. A chunk that may continue also returns the state it stops in: a line
    /// comment, string, escape or word, and the bytes of a pattern the chunk end may have cut.
    /// The next chunk lexes those bytes again with its own, so it colours as the whole line
    /// would; tokens of the held bytes come back only for the bytes of the new chunk.
    #[must_use]
    pub fn chunk(&self, chunk: &str, state: ChunkState, last: bool) -> (Vec<Token>, ChunkState) {
        let Some((held, after_word)) = self.held(state) else {
            let feed = Feed {
                last,
                lead: 0,
                held: 0,
            };
            return self.lex(chunk, state, feed);
        };
        let inside = match state.inside {
            Inside::Opening(_) => Inside::Code,
            other => other,
        };
        let lead = if after_word { WORD_BYTE } else { "" };
        let joined = [lead, held, chunk].concat();
        let resumed = ChunkState {
            inside,
            mid_word: false,
            partial: 0,
        };
        let feed = Feed {
            last,
            lead: lead.len(),
            held: held.len(),
        };
        let (tokens, next) = self.lex(&joined, resumed, feed);
        let cut = joined.len() - chunk.len();
        let tokens = tokens
            .into_iter()
            .filter(|token| token.range.end > cut)
            .map(|token| Token {
                range: token.range.start.saturating_sub(cut)..token.range.end - cut,
                kind: token.kind,
            })
            .take(MAX_TOKENS)
            .collect();
        (tokens, next)
    }

    /// The bytes the chunk before held back, and whether a word byte precedes them; None when
    /// nothing is held.
    fn held(&self, state: ChunkState) -> Option<(&str, bool)> {
        let end = usize::from(state.partial);
        let (text, after_word) = match state.inside {
            Inside::Opening(index) => (self.holds.text(index)?, state.mid_word),
            Inside::Block(index) => (&self.blocks.get(usize::from(index))?.1, false),
            Inside::Delimited {
                index,
                escaped: false,
            } => (&self.delimiters.get(usize::from(index))?.close, false),
            _ => return None,
        };
        text.get(..end)
            .filter(|_| end > 0)
            .map(|held| (held, after_word))
    }

    /// Lexes `text` from `state`; the held bytes get room in the token cap.
    pub(super) fn lex(
        &self,
        text: &str,
        state: ChunkState,
        feed: Feed,
    ) -> (Vec<Token>, ChunkState) {
        let Feed { last, lead, held } = feed;
        let mut run = Run::new(text, last);
        if held > 0 {
            run.cap += held;
        }
        if !last {
            run.tail_from = text
                .len()
                .saturating_sub(self.holds.longest().saturating_sub(1));
            let starts_a_step = state.mid_word && lead == 0 && word_len(text) == 0;
            if starts_a_step && run.tail_from == 0 && !text.is_empty() {
                run.marks.push(0);
            }
        }
        let step = if lead > 0 {
            Step::At(lead)
        } else {
            self.resume(&mut run, state)
        };
        let inside = match step {
            Step::At(at) if last => self.scan::<false>(&mut run, at),
            Step::At(at) => self.scan::<true>(&mut run, at),
            Step::Done(inside) => inside,
        };
        let next = self.ending(&run, inside, state.mid_word);
        (run.tokens, next)
    }

    /// The state a chunk hands on: where it stopped, and what a following chunk must finish.
    fn ending(&self, run: &Run<'_>, inside: Inside, was_mid_word: bool) -> ChunkState {
        if run.last {
            return ChunkState {
                inside,
                mid_word: false,
                partial: 0,
            };
        }
        if let Some((index, partial, mid_word)) = self.opening(run, was_mid_word) {
            return ChunkState {
                inside: Inside::Opening(index),
                mid_word,
                partial,
            };
        }
        let mid_word = inside == Inside::Code
            && if run.line.is_empty() {
                was_mid_word
            } else {
                word_before(run.line, run.line.len())
            };
        let partial = u16::try_from(run.closing).unwrap_or(0);
        ChunkState {
            inside,
            mid_word,
            partial,
        }
    }

    /// Finds the earliest step start where a comment or string opener could still be completing:
    /// the opener index, the bytes seen and whether a word byte precedes them. The opener must
    /// fit the word rule there (a chunk that began mid-word has a word byte behind its start).
    fn opening(&self, run: &Run<'_>, was_mid_word: bool) -> Option<(u16, u16, bool)> {
        let text = run.line;
        run.marks.iter().find_map(|&start| {
            let tail = text.get(start..)?;
            let behind = if start == 0 {
                was_mid_word
            } else {
                word_before(text, start)
            };
            let found = self.holds.find(tail, behind)?;
            Some((found, u16::try_from(tail.len()).ok()?, behind))
        })
    }
}
