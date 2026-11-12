//! Syntax colours of the Stream's text cards (D38.3): every hunk snippet a card shows is parsed
//! as a document of its own, and a drawn line takes its colour runs from that snippet. The Stream
//! builds a card's `CardSyntax` in one background job; the card rows only ask it for colours.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::HighlightStyle;
use gpui_kit::base::input::HighlightStyleResolver;
use gpui_kit::component::highlighter::HighlightTheme;

use super::line::half_tones;
use crate::compare_diff::body::{PairSyntax, SideSyntax, parse_side};
use crate::compare_diff::hooks::{Card, CardRow};
use crate::compare_diff::stream_model::StreamModel;
use crate::tokens::{Theme, TintLook, on_tint};

/// A colour run of a drawn line: the bytes of its text, and the style that colours them.
pub type TextRun = (Range<usize>, HighlightStyle);

/// The card a parse belongs to: the listed item, the Stream generation its card arrived under,
/// and the card itself. A finished parse is kept only while that very card is held (D38.3).
#[derive(Clone)]
pub struct CardKey {
    /// The listed item's id.
    id: u64,
    /// The generation the card was sent under.
    generation: u64,
    /// The card whose rows were parsed.
    card: Arc<Card>,
}

impl CardKey {
    /// The key of item `id`'s card `card`, read under `generation`.
    #[must_use]
    pub fn new(id: u64, generation: u64, card: &Arc<Card>) -> Self {
        Self {
            id,
            generation,
            card: Arc::clone(card),
        }
    }

    /// Whether both name the same item under the same generation with the very same card.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        self.id == other.id
            && self.generation == other.generation
            && Arc::ptr_eq(&self.card, &other.card)
    }

    /// Whether `model` still holds this very card, whatever generation it is listed under now.
    #[must_use]
    pub fn held(&self, model: &StreamModel) -> bool {
        model
            .card(self.id)
            .is_some_and(|card| Arc::ptr_eq(card, &self.card))
    }

    /// Whether the card was read under an older generation than the Stream is on.
    #[must_use]
    pub fn stale(&self, generation: u64) -> bool {
        self.generation != generation
    }

    /// The listed item this card belongs to.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The card whose rows were parsed.
    #[must_use]
    pub fn card(&self) -> &Arc<Card> {
        &self.card
    }
}

/// The lines of one hunk snippet, as the card lists them. Each side is parsed as a document of
/// its own, so a snippet is two documents, never one.
#[derive(Default)]
struct Snippet {
    /// The hunk these lines belong to, as the card numbers it.
    hunk: u32,
    /// The snippet's left lines, in card order.
    left: Vec<String>,
    /// The snippet's right lines, in card order.
    right: Vec<String>,
}

/// Where one card row sits in the card's snippets: the snippet of its hunk, and the index of its
/// line inside that snippet on each side.
#[derive(Clone, Copy)]
struct Place {
    snippet: usize,
    left: Option<u32>,
    right: Option<u32>,
}

/// One text card parsed for its colour runs: the snippet of every hunk the card shows and where
/// each card row sits inside it. The Stream builds it off the UI thread, once per card.
pub struct CardSyntax {
    /// The parsed snippets, in the order the card's rows bring up their hunks.
    hunks: Vec<PairSyntax>,
    /// One place per card row, in the card's own row order, so the draw never counts. A row that
    /// shows no code — a gap or a "more" note — has none.
    places: Vec<Option<Place>>,
}

impl CardSyntax {
    /// Parses the text card `card`, of a pair whose name is `path`: a small document per hunk
    /// snippet on each side, and the place of every card row, in one pass. Blocks for the whole
    /// card: run it off the UI thread.
    #[must_use]
    pub fn parse(path: &str, card: &Card, cap: u64) -> Self {
        crate::languages::register_languages();
        let mut snippets: Vec<Snippet> = Vec::new();
        let mut places: Vec<Option<Place>> = Vec::with_capacity(card.rows.len());
        for row in &card.rows {
            let CardRow::Line {
                hunk, left, right, ..
            } = row
            else {
                places.push(None);
                continue;
            };
            open_snippet(&mut snippets, *hunk);
            let index = snippets.len() - 1;
            let snippet = &mut snippets[index];
            places.push(Some(Place {
                snippet: index,
                left: snippet_line(&mut snippet.left, left.as_ref()),
                right: snippet_line(&mut snippet.right, right.as_ref()),
            }));
        }
        let hunks = snippets
            .iter()
            .map(|snippet| PairSyntax {
                left: parse_side(path, &snippet.left, cap),
                right: parse_side(path, &snippet.right, cap),
            })
            .collect();
        Self { hunks, places }
    }
}

/// Opens the snippet of hunk `hunk` when the card has moved on to another one: the rows of a hunk
/// come together, and a gap between them starts a new snippet.
fn open_snippet(snippets: &mut Vec<Snippet>, hunk: u32) {
    if snippets.last().is_none_or(|last| last.hunk != hunk) {
        snippets.push(Snippet {
            hunk,
            left: Vec::new(),
            right: Vec::new(),
        });
    }
}

/// The index `text` takes in a snippet's lines, after adding it; a row with no text on that side
/// adds nothing and has no index there.
fn snippet_line(lines: &mut Vec<String>, text: Option<&String>) -> Option<u32> {
    let text = text?;
    let at = lines.len();
    lines.push(text.clone());
    u32::try_from(at).ok()
}

/// What one card paints its lines with (D38.3): its parsed snippets and the theme's highlight
/// colours, read at draw time so that a theme change recolours without a re-parse.
#[derive(Clone, Copy)]
pub struct CardLook<'a> {
    /// The card's parsed hunk snippets.
    pub syntax: &'a CardSyntax,
    /// F3's syntax highlight colours the runs are resolved against.
    pub colours: &'a HighlightTheme,
}

/// One side of a card line asked about: the text it shows, its index inside the snippet, and
/// whether its text column is tinted.
struct SideLine<'a> {
    text: Option<&'a str>,
    at: Option<u32>,
    tinted: bool,
}

impl<'a> CardLook<'a> {
    /// The look for a frame, or `None` until both the card's parse and the colours exist.
    #[must_use]
    pub fn new(
        syntax: Option<&'a CardSyntax>,
        colours: Option<&'a HighlightTheme>,
    ) -> Option<Self> {
        Some(Self {
            syntax: syntax?,
            colours: colours?,
        })
    }

    /// The colour runs of both sides of card row `at` of `rows`, in bytes of each side's text;
    /// empty for a row that shows no code, a side with no text, no place or no parse, or a text
    /// that is not that line of the snippet. Each side colours from its own hunk snippet.
    pub fn line_runs(&self, t: &Theme, rows: &[CardRow], at: u32) -> (Vec<TextRun>, Vec<TextRun>) {
        let index = at as usize;
        let (
            Some(place),
            Some(CardRow::Line {
                row, left, right, ..
            }),
        ) = (
            self.syntax.places.get(index).and_then(Option::as_ref),
            rows.get(index),
        )
        else {
            return (Vec::new(), Vec::new());
        };
        let Some(snippet) = self.syntax.hunks.get(place.snippet) else {
            return (Vec::new(), Vec::new());
        };
        let (lt, rt) = half_tones(t, *row);
        let left = SideLine {
            text: left.as_deref(),
            at: place.left,
            tinted: lt.is_some(),
        };
        let right = SideLine {
            text: right.as_deref(),
            at: place.right,
            tinted: rt.is_some(),
        };
        (
            self.side_runs(t, snippet.left.as_ref(), left),
            self.side_runs(t, snippet.right.as_ref(), right),
        )
    }

    /// The runs of `line` in `side`, in the on-tint variant when its column is tinted (D38.4);
    /// empty when that side shows no text, has no index here or stayed plain.
    fn side_runs(&self, t: &Theme, side: Option<&SideSyntax>, line: SideLine<'_>) -> Vec<TextRun> {
        let (Some(side), Some(text), Some(at)) = (side, line.text, line.at) else {
            return Vec::new();
        };
        let resolver = TintedColours {
            colours: self.colours,
            tinted: line.tinted,
            look: TintLook {
                mode: t.mode,
                syntax: t.syntax,
            },
        };
        side.line_runs((at, text), &resolver)
    }
}

/// F3's highlight colours, moved by `on_tint` on a tinted card line (D38.4). Syntax backgrounds
/// are dropped, as in the file body: the row wash and the accent bar own the background.
struct TintedColours<'a> {
    colours: &'a HighlightTheme,
    tinted: bool,
    look: TintLook,
}

impl HighlightStyleResolver for TintedColours<'_> {
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
