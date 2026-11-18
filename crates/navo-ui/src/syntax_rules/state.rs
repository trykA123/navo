//! Chunk state for the rule lexer (D7): how the state `navo-syntax` reports at a chunk end is
//! packed into the line-lexer contract's `u32` and handed to the next chunk of the same line.

use navo_syntax::{ChunkState, Inside, MAX_PATTERN_BYTES};
use sidewise_kit::languages::lines::{self, LineState, Span};

use super::RuleLexer;

const BLOCK_TAG: u32 = 1;
const DELIMITED_TAG: u32 = 2;
const COMMENT_TAG: u32 = 3;
const ESCAPED_TAG: u32 = 4;
const OPENING_TAG: u32 = 5;
const TAG_SHIFT: u32 = 16;
const TAG_MASK: u32 = 0xF;
/// The chunk ended inside a word.
const MID_WORD: u32 = 0x10_0000;
/// Where the count of closer or opener bytes at the chunk end sits, and how many bits it has.
const PARTIAL_SHIFT: u32 = 21;
const PARTIAL_MASK: u32 = 0x7FF;
// A held pattern is shorter than the longest pattern a language may have; it must fit the field.
const _: () = assert!(MAX_PATTERN_BYTES as u32 - 1 <= PARTIAL_MASK);

/// Packs a chunk state into the line-lexer contract's `u32`; the layout is tag, then index.
pub(super) fn pack(state: ChunkState) -> LineState {
    let (tag, index) = match state.inside {
        Inside::Code => (0, 0),
        Inside::Opening(index) => (OPENING_TAG, index),
        Inside::LineComment => (COMMENT_TAG, 0),
        Inside::Block(index) => (BLOCK_TAG, index),
        Inside::Delimited {
            index,
            escaped: false,
        } => (DELIMITED_TAG, index),
        Inside::Delimited {
            index,
            escaped: true,
        } => (ESCAPED_TAG, index),
    };
    let word = if state.mid_word { MID_WORD } else { 0 };
    debug_assert!(
        u32::from(state.partial) <= PARTIAL_MASK,
        "a held pattern fits its field"
    );
    let partial = (u32::from(state.partial) & PARTIAL_MASK) << PARTIAL_SHIFT;
    LineState((tag << TAG_SHIFT) | u32::from(index) | word | partial)
}

/// Unpacks a line-lexer state; an unknown tag or value becomes `Code`.
pub(super) fn unpack(state: LineState) -> ChunkState {
    let index = u16::try_from(state.0 & 0xFFFF).unwrap_or(0);
    let inside = match (state.0 >> TAG_SHIFT) & TAG_MASK {
        BLOCK_TAG => Inside::Block(index),
        DELIMITED_TAG => Inside::Delimited {
            index,
            escaped: false,
        },
        COMMENT_TAG => Inside::LineComment,
        ESCAPED_TAG => Inside::Delimited {
            index,
            escaped: true,
        },
        OPENING_TAG => Inside::Opening(index),
        _ => Inside::Code,
    };
    let partial = match inside {
        Inside::Block(_) | Inside::Opening(_) | Inside::Delimited { escaped: false, .. } => {
            u16::try_from((state.0 >> PARTIAL_SHIFT) & PARTIAL_MASK).unwrap_or(0)
        }
        _ => 0,
    };
    ChunkState {
        inside,
        mid_word: matches!(inside, Inside::Code | Inside::Opening(_)) && state.0 & MID_WORD != 0,
        partial,
    }
}

impl RuleLexer {
    /// Lexes one chunk. `ends_line` is true when no later chunk continues the line.
    ///
    /// The lexer itself reports where a chunk that may continue stops (comment, string, escape,
    /// word), so the next chunk lexes as the whole line would.
    pub(super) fn lex_chunk(
        &self,
        chunk: &str,
        state: LineState,
        ends_line: bool,
    ) -> (Vec<Span>, LineState) {
        let (tokens, next) = self.lexer.chunk(chunk, unpack(state), ends_line);
        let spans = tokens
            .into_iter()
            .take(lines::MAX_SPANS)
            .map(|token| (token.range, token.kind.highlight_name()))
            .collect();
        (spans, pack(next))
    }

    /// Returns the state a following line starts in: a comment, an escape and a word end with
    /// their line, and a string goes on only when its delimiter spans lines.
    pub(super) fn end_of_line(&self, state: LineState) -> LineState {
        let carried = unpack(state);
        match carried.inside {
            Inside::Block(index) => pack(ChunkState {
                inside: Inside::Block(index),
                ..ChunkState::default()
            }),
            Inside::Delimited { index, .. } if self.runs_over_lines(index) => pack(ChunkState {
                inside: Inside::Delimited {
                    index,
                    escaped: false,
                },
                ..ChunkState::default()
            }),
            _ => LineState(0),
        }
    }

    /// True when delimiter `index` has a close and may span line breaks.
    fn runs_over_lines(&self, index: u16) -> bool {
        self.language
            .delimiters
            .get(usize::from(index))
            .is_some_and(|d| d.multiline && !d.close.is_empty())
    }
}
