use std::sync::Arc;

use rusty_core::Location;

use super::{FileHooks, FileText, Head, SaveError, Viewer};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const TEXT: &str = include_str!("../../../../../docs/user-guide.md");
const NAME: &str = "navo-user-guide.md";
const HEAD: usize = 8 * 1024;

fn hooks() -> FileHooks {
    let head = |_: &Location| {
        let end = TEXT.len().min(HEAD);
        Ok(Head {
            bytes: TEXT.as_bytes()[..end].to_vec(),
            len: u64::try_from(TEXT.len()).unwrap_or(u64::MAX),
        })
    };
    let read = |_: &Location| {
        Ok(FileText {
            read_only: true,
            ..FileText::plain(TEXT)
        })
    };
    let text_only = || rusty_l10n::tr("viewer-guide-formatted-only");
    FileHooks {
        head: Arc::new(head),
        read: Arc::new(read),
        read_as: Arc::new(move |location, _| read(location)),
        write: Arc::new(|_, _| Err(SaveError::Failed(rusty_l10n::tr("viewer-guide-read-only")))),
        paged: Arc::new(move |_| Err(text_only())),
        paged_as: Arc::new(move |_, _| Err(text_only())),
        ..FileHooks::none()
    }
}

impl Viewer {
    pub(super) fn shown_title(&self) -> SharedString {
        match self.guide {
            Some(_) => rusty_l10n::tr("viewer-guide-title").into(),
            None => self.title.clone(),
        }
    }

    pub(super) fn is_guide(&self) -> bool {
        self.guide.is_some()
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn open_user_guide(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.is_some() {
            return;
        }
        let Ok(location) = Location::new(NAME) else {
            return;
        };
        let mut viewer = Viewer::new(location, cx.focus_handle());
        viewer.guide = Some(hooks());
        viewer.opening = Some(self.open_file(&viewer, window, cx));
        self.show_viewer(viewer, window, cx);
    }
}
