use rusty_structured::markdown::{self, MAX_SOURCE, Refusal};

use super::{Declined, Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::markdown::{MarkdownBody, MarkdownFile};

const MARKDOWN: u8 = 70;
const EXTENSIONS: [&str; 2] = ["md", "markdown"];
const TOO_LARGE: &str = "Large Markdown: shown as text.";
const TOO_MANY_BLOCKS: &str = "Markdown with over 200,000 blocks: shown as text.";
const TOO_MANY_SPANS: &str = "Markdown with over 2,000,000 styled runs: shown as text.";
const TOO_MANY_FOOTNOTES: &str = "Too many footnotes for the rendered view";

pub(super) struct MarkdownKind;

impl MarkdownKind {
    fn is_markdown(sniff: &Sniff<'_>) -> bool {
        named(sniff.name, &EXTENSIONS) && !sniff.looks_binary()
    }

    fn fits(sniff: &Sniff<'_>) -> bool {
        sniff.len <= MAX_SOURCE as u64
    }
}

fn note(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::TooLarge => TOO_LARGE,
        Refusal::TooManyBlocks => TOO_MANY_BLOCKS,
        Refusal::TooManySpans => TOO_MANY_SPANS,
        Refusal::TooManyFootnotes => TOO_MANY_FOOTNOTES,
    }
}

impl ViewerKind for MarkdownKind {
    fn id(&self) -> &'static str {
        "markdown"
    }

    fn name(&self) -> &'static str {
        "Markdown"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if Self::is_markdown(sniff) && Self::fits(sniff) {
            MARKDOWN
        } else {
            0
        }
    }

    fn declined(&self, sniff: &Sniff<'_>) -> Option<&'static str> {
        (Self::is_markdown(sniff) && !Self::fits(sniff)).then_some(TOO_LARGE)
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let Load {
            hooks,
            location,
            sniff,
            ..
        } = load;
        let file = (hooks.read)(location)?;
        load.ensure_open()?;
        let doc = match markdown::parse(&file.text) {
            Ok(doc) => doc,
            Err(refusal) => return Ok(Box::new(Declined(note(refusal)))),
        };
        load.ensure_open()?;
        Ok(Box::new(MarkdownFile {
            doc,
            len: sniff.len,
        }))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let file = attach
            .loaded
            .downcast::<MarkdownFile>()
            .map_err(|_| "The markdown view got data it cannot show.".to_owned())?;
        Ok(Box::new(MarkdownBody::new(*file, attach.host, cx)))
    }
}
