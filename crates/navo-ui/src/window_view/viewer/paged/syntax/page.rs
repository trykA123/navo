//! Colours the rows of one paged text page with a line lexer, one chunk per row.

use sidewise_kit::languages::lines::{LineLexer, LineState};
use sidewise_kit::languages::lines::{MAX_PAGE_BYTES, MAX_PAGE_SPANS};

use super::super::lines::TextLine;

/// Lexer states recorded while one page was coloured.
pub(in crate::window_view::viewer::paged) struct PageLex {
    /// The state at each lexed row's start offset.
    pub starts: Vec<(u64, LineState)>,
    /// The state at the `next` offset of the last lexed row.
    pub end: (u64, LineState),
}

/// Colours the rows of one page in order, each row as one chunk of a logical line.
///
/// `file_len` is the source length in bytes: a row ending there ends its line too.
/// Lexing stops at a row that would pass `MAX_PAGE_BYTES` of text, or at `MAX_PAGE_SPANS` spans.
pub(in crate::window_view::viewer::paged) fn lex_page(
    lexer: &dyn LineLexer,
    lines: &mut [TextLine],
    start: LineState,
    file_len: u64,
) -> PageLex {
    let mut state = start;
    let mut bytes = 0;
    let mut spans = 0;
    let mut starts = Vec::with_capacity(lines.len());
    let mut end = (lines.first().map_or(0, |line| line.start), start);
    for line in lines.iter_mut() {
        if bytes + line.text.len() > MAX_PAGE_BYTES || spans >= MAX_PAGE_SPANS {
            break;
        }
        starts.push((line.start, state));
        // A row that ends its line, or the file, is a chunk that ends its line; a cut row is not.
        let ends_line = line.next > line.end || line.end >= file_len;
        let (mut found, next) = if ends_line {
            lexer.lex_last(&line.text, state)
        } else {
            lexer.lex(&line.text, state)
        };
        found.truncate(MAX_PAGE_SPANS - spans);
        spans += found.len();
        bytes += line.text.len();
        line.spans = found;
        state = if ends_line {
            lexer.line_end(next)
        } else {
            next
        };
        end = (line.next, state);
    }
    PageLex { starts, end }
}
