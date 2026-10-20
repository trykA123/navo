use super::markdown::markdown_extension;
use super::{EDIT_LIMIT, Load, Loaded, Slot, Sniff, ViewerKind};
use crate::languages::detect;
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::editor::{EditorBody, EditorFile, SplitChoice};
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::{Paged, PagedKind};

const TEXT: u8 = 50;
const BINARY: u8 = 1;
const NEEDS_WHOLE_FILE: &str =
    "This encoding needs the whole file to read text, so it opens as Hex.";

enum TextLoad {
    Editor(EditorFile),
    Paged(Paged),
}

pub(super) struct Text;

pub(in crate::window_view::viewer) fn paged_text(paged: Paged) -> Loaded {
    Box::new(TextLoad::Paged(paged))
}

impl ViewerKind for Text {
    fn id(&self) -> &'static str {
        "text"
    }

    fn name(&self) -> &'static str {
        "Text"
    }

    fn slot(&self) -> Slot {
        Slot::Text
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.looks_binary() { BINARY } else { TEXT }
    }

    fn editable(&self, sniff: &Sniff<'_>) -> bool {
        !sniff.looks_binary() && sniff.len <= EDIT_LIMIT
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let Load {
            hooks,
            location,
            sniff,
            encoding,
            ..
        } = load;
        let loaded = if self.editable(sniff) {
            TextLoad::Editor(EditorFile {
                file: hooks.read_with(location, encoding)?,
                language: detect(sniff.name, sniff.head),
                split: markdown_extension(sniff.name).map(|_| SplitChoice { is_shown: true }),
            })
        } else {
            TextLoad::Paged(hooks.paged_with(location, encoding)?)
        };
        Ok(Box::new(loaded))
    }

    fn attach(
        &self,
        attach: Attach,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let loaded = attach
            .loaded
            .downcast::<TextLoad>()
            .map_err(|_| "The text view got data it cannot show.".to_owned())?;
        Ok(match *loaded {
            TextLoad::Editor(file) => {
                let body = EditorBody::new(file, attach.host, window, cx);
                if attach.edit {
                    body.start_editing(cx);
                }
                Box::new(body)
            }
            TextLoad::Paged(paged) => {
                let whole = paged.source.layout().needs_whole_file();
                let kind = if whole {
                    PagedKind::Hex
                } else {
                    PagedKind::Text
                };
                let body = PagedBody::new(attach.host, paged, kind, cx);
                if whole {
                    body.set_note(NEEDS_WHOLE_FILE.into(), cx);
                }
                Box::new(body)
            }
        })
    }

    fn reuse(&self, body: &dyn ViewerBody, sniff: &Sniff<'_>, cx: &mut App) -> bool {
        let Some(paged) = PagedBody::of(body)
            .filter(|_| !self.editable(sniff))
            .filter(|paged| !paged.needs_whole_file(cx))
        else {
            return false;
        };
        paged.show_as(PagedKind::Text, cx);
        true
    }
}
