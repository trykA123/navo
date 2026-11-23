use super::markdown::markdown_extension;
use super::{EDIT_LIMIT, Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::editor::{EditorBody, EditorFile, SplitChoice};
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::swap::{EditorLater, SWAP_FROM};
use crate::window_view::viewer::syntax::{Pick, choose};
use crate::window_view::viewer::{Paged, PagedKind};
use sidewise_kit::languages::lines::LineLexer;
use std::sync::Arc;

const TEXT: u8 = 50;
const BINARY: u8 = 1;
const NEEDS_WHOLE_FILE: &str =
    "This encoding needs the whole file to read text, so it opens as Hex.";

enum TextLoad {
    Editor(EditorFile),
    /// The paged view, its line lexer, and the editor to swap in later for a large editable file.
    Paged(Paged, Option<Arc<dyn LineLexer>>, Option<EditorLater>),
}

pub(super) struct Text;

pub(in crate::window_view::viewer) fn paged_text(paged: Paged) -> Loaded {
    Box::new(TextLoad::Paged(paged, None, None))
}

/// Takes the editor still to build out of a large file's paged load, editing from the start
/// when `edit` is set; `None` for any other load.
pub(in crate::window_view::viewer) fn take_later(
    loaded: &mut Result<Loaded, String>,
    edit: bool,
) -> Option<EditorLater> {
    let TextLoad::Paged(_, _, later) = loaded.as_mut().ok()?.downcast_mut::<TextLoad>()? else {
        return None;
    };
    let mut later = later.take()?;
    later.edit = edit;
    Some(later)
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

    /// Reads a small editable file whole for the editor; any other file opens paged, and a large
    /// editable one also carries the editor to swap in once its text is read.
    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let Load {
            hooks,
            location,
            sniff,
            encoding,
            language,
            ..
        } = load;
        let pick = Pick {
            name: sniff.name,
            head: sniff.head,
            choice: language,
            len: Some(sniff.len),
        };
        let editable = self.editable(sniff);
        let split = markdown_extension(sniff.name).map(|_| SplitChoice { is_shown: true });
        if !editable || sniff.len > SWAP_FROM {
            let paged = hooks.paged_with(location, encoding)?;
            if !editable || !paged.source.layout().needs_whole_file() {
                let later = editable.then(|| EditorLater {
                    highlight: choose(&pick),
                    split,
                    edit: false,
                });
                let lexer = choose(&pick).paged_lexer();
                return Ok(Box::new(TextLoad::Paged(paged, lexer, later)));
            }
        }
        Ok(Box::new(TextLoad::Editor(EditorFile {
            file: hooks.read_with(location, encoding)?,
            highlight: choose(&pick),
            split,
        })))
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
            TextLoad::Paged(paged, lexer, _) => {
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
                body.set_lexer(lexer, cx);
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
