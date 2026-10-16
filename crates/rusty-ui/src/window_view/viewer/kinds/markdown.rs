mod nesting;

use super::{Declined, EDIT_LIMIT, Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::markdown::{MarkdownBody, MarkdownFile};

const MARKDOWN: u8 = 70;
const EXTENSIONS: [&str; 2] = ["md", "markdown"];
const TOO_LARGE: &str = "Over 16 MB: shown as text, not rendered.";
const TOO_DEEP: &str = "Nested too deeply to render safely: shown as text.";

pub(super) struct MarkdownKind;

impl MarkdownKind {
    fn is_markdown(sniff: &Sniff<'_>) -> bool {
        named(sniff.name, &EXTENSIONS) && !sniff.looks_binary()
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
        if Self::is_markdown(sniff) && sniff.len <= EDIT_LIMIT {
            MARKDOWN
        } else {
            0
        }
    }

    fn declined(&self, sniff: &Sniff<'_>) -> Option<&'static str> {
        (Self::is_markdown(sniff) && sniff.len > EDIT_LIMIT).then_some(TOO_LARGE)
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let Load {
            hooks,
            location,
            sniff,
            ..
        } = load;
        let file = (hooks.read)(location)?;
        if nesting::is_too_deep(&file.text) {
            return Ok(Box::new(Declined(TOO_DEEP)));
        }
        Ok(Box::new(MarkdownFile {
            text: file.text,
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
