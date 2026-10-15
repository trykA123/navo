use rusty_core::Location;

use super::{EDIT_LIMIT, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::editor::EditorBody;
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::{FileHooks, FileText, Paged, PagedKind};

const TEXT: u8 = 50;
const BINARY: u8 = 1;

enum TextLoad {
    Editor(FileText),
    Paged(Paged),
}

pub(super) struct Text;

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

    fn load(
        &self,
        hooks: &FileHooks,
        location: &Location,
        sniff: &Sniff<'_>,
    ) -> Result<Loaded, String> {
        let loaded = if self.editable(sniff) {
            TextLoad::Editor((hooks.read)(location)?)
        } else {
            TextLoad::Paged((hooks.paged)(location)?)
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
                Box::new(PagedBody::new(attach.host, paged, PagedKind::Text, cx))
            }
        })
    }

    fn reuse(&self, body: &dyn ViewerBody, sniff: &Sniff<'_>, cx: &mut App) -> bool {
        let Some(paged) = PagedBody::of(body).filter(|_| !self.editable(sniff)) else {
            return false;
        };
        paged.show_as(PagedKind::Text, cx);
        true
    }
}
