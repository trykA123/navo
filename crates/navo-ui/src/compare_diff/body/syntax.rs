//! Syntax colours of the file diff (D38): each side's text is one document parsed by F3's
//! tree-sitter highlighter; the body asks it for the colour runs of the lines it draws.

use std::ops::Range;

use gpui_kit::HighlightStyle;
use gpui_kit::base::input::HighlightStyleResolver;
use gpui_kit::component::Rope;
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};

use super::cut::LineAt;
use super::text::text_tone;
use super::{BodyLook, Side};
use crate::compare_diff::long::is_long;
use crate::tokens::{TintLook, on_tint};

/// The language `languages::detect` names for text with no grammar.
const PLAIN: &str = "text";

/// How many bytes of a side's first lines the language sniffer reads.
const HEAD: usize = 1024;

/// Parses one side of a pair named `path`, whose lines are `lines`, as a document of its own:
/// the language comes from the file name and the head of the text, and `lines` is parsed to the
/// end. `None` for plain text or a side over `cap` bytes (F3's `EDIT_LIMIT`). Blocks for the
/// whole parse: call it off the UI thread.
pub(crate) fn parse_side(path: &str, lines: &[String], cap: u64) -> Option<SideSyntax> {
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let mut head = String::new();
    for line in lines {
        if head.len() >= HEAD {
            break;
        }
        head.push_str(line);
        head.push('\n');
    }
    let language = crate::languages::detect(name, head.as_bytes());
    SideSyntax::parse(language, lines, cap)
}

/// One side's text parsed as a single document, and where each of its lines starts.
pub struct SideSyntax {
    highlighter: SyntaxHighlighter,
    /// The byte offset of each line in the document, plus the end.
    starts: Vec<usize>,
}

impl SideSyntax {
    /// Parses `lines`, joined by `\n`, as `language` to the end. `None` for plain text or a side
    /// over `cap` bytes (F3's `EDIT_LIMIT`). Blocks for the whole parse: call it off the UI thread.
    #[must_use]
    pub fn parse(language: &str, lines: &[String], cap: u64) -> Option<Self> {
        let bytes: usize = lines.iter().map(|line| line.len() + 1).sum();
        if language == PLAIN || u64::try_from(bytes).map_or(true, |bytes| bytes > cap) {
            return None;
        }
        let mut starts = Vec::with_capacity(lines.len() + 1);
        let mut document = String::with_capacity(bytes);
        for line in lines {
            starts.push(document.len());
            document.push_str(line);
            document.push('\n');
        }
        starts.push(document.len());
        let mut highlighter = SyntaxHighlighter::new(language);
        highlighter.update(None, &Rope::from(document.as_str()), None);
        Some(Self {
            highlighter,
            starts,
        })
    }

    /// The colour runs of this side's line `at`, whose text is `text`, in bytes of `text`; empty
    /// when `text` is not that line of the parsed document (it never colours the wrong bytes).
    pub fn line_runs(
        &self,
        (at, text): (u32, &str),
        colours: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let at = at as usize;
        let (Some(&start), Some(&next)) = (self.starts.get(at), self.starts.get(at + 1)) else {
            return Vec::new();
        };
        if next - start != text.len() + 1 {
            return Vec::new();
        }
        let line = start..start + text.len();
        self.highlighter
            .styles(&line, colours)
            .into_iter()
            .filter_map(|(range, style)| {
                let run = line_run(range, start, text.len());
                (!run.is_empty() && style != HighlightStyle::default()).then_some((run, style))
            })
            .collect()
    }
}

/// The run `range` of the parsed document in bytes of the line that starts at `start` and is
/// `len` bytes long; empty when the run lies outside the line.
fn line_run(range: Range<usize>, start: usize, len: usize) -> Range<usize> {
    let from = range.start.saturating_sub(start).min(len);
    let to = range.end.saturating_sub(start).min(len);
    from..to
}

/// Both sides' parses of one opened text; a side is `None` when it stays plain.
#[derive(Default)]
pub struct PairSyntax {
    /// The left file's parse, or `None` when it stays plain.
    pub left: Option<SideSyntax>,
    /// The right file's parse, or `None` when it stays plain.
    pub right: Option<SideSyntax>,
}

/// The parsed pair and the theme's highlight colours (as F3 gets them) for one frame.
#[derive(Clone, Copy)]
pub struct SyntaxLook<'a> {
    /// Both sides' parses of the opened text.
    pub pair: &'a PairSyntax,
    /// The theme's syntax highlight colours the runs are resolved against.
    pub colours: &'a HighlightTheme,
}

impl<'a> SyntaxLook<'a> {
    /// The look for a frame, or `None` until both the parse and the colours exist.
    #[must_use]
    pub fn new(pair: Option<&'a PairSyntax>, colours: Option<&'a HighlightTheme>) -> Option<Self> {
        Some(Self {
            pair: pair?,
            colours: colours?,
        })
    }
}

/// The syntax colour runs of `at`'s line in bytes of its text; empty with no parse for its side.
/// A line over `LONG_LINE` stays plain: it is drawn whole, and its runs would cost too much.
pub(super) fn syntax_runs(look: &BodyLook, at: &LineAt) -> Vec<(Range<usize>, HighlightStyle)> {
    let (Some(syntax), Some(line)) = (look.syntax, at.line) else {
        return Vec::new();
    };
    let side = match at.side {
        Side::Left => syntax.pair.left.as_ref(),
        Side::Right => syntax.pair.right.as_ref(),
    };
    let Some(side) = side.filter(|_| !is_long(line.1)) else {
        return Vec::new();
    };
    let t = look.theme;
    let resolver = OnTint {
        colours: syntax.colours,
        tinted: text_tone(t, at.kind, at.side).is_some(),
        look: TintLook {
            mode: t.mode,
            syntax: t.syntax,
        },
    };
    side.line_runs(line, &resolver)
}

/// F3's highlight colours, moved by `on_tint` on a tinted line (D38.4). Syntax backgrounds are
/// dropped: the row wash and the changed-word fill own the background.
struct OnTint<'a> {
    colours: &'a HighlightTheme,
    tinted: bool,
    look: TintLook,
}

impl HighlightStyleResolver for OnTint<'_> {
    /// The style of highlight `name`, or `None` when the theme has none.
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        let mut style = self.colours.style(name)?;
        style.color = style
            .color
            .map(|base| on_tint(name, base, self.tinted, self.look));
        style.background_color = None;
        Some(style)
    }
}

#[cfg(test)]
mod tests;
