use super::*;

pub(super) const VIEWER_MENUS: [ViewerMenuShows; 8] = [
    ViewerMenuShows::Text,
    ViewerMenuShows::Tree,
    ViewerMenuShows::ViewText,
    ViewerMenuShows::Encoding,
    ViewerMenuShows::ViewAutomotive,
    ViewerMenuShows::ViewElf,
    ViewerMenuShows::ViewTrace,
    ViewerMenuShows::ViewMarkdown,
];

impl Gallery {
    pub(super) fn sample(&self) -> Option<&dyn ViewerBody> {
        match self.page {
            Page::Tree => self.tree.as_deref(),
            Page::Table => self.table.as_deref(),
            Page::Markdown => self.markdown.as_deref(),
            Page::MarkdownPreview => self.markdown_preview.as_deref(),
            Page::Code => self.code.as_deref(),
            Page::MemoryImage => self.memory[0].as_deref(),
            Page::MemoryDrawers => self.memory[1].as_deref(),
            Page::ElfSymbols => self.elf[0].as_deref(),
            Page::ElfDrawers => self.elf[1].as_deref(),
            Page::MapTables => self.map[0].as_deref(),
            Page::MapDrawers => self.map[1].as_deref(),
            Page::TraceTable => self.trace[0].as_deref(),
            Page::TraceDrawers => self.trace[1].as_deref(),
            _ => None,
        }
    }

    pub(super) fn pdf_sample(&self) -> Option<&Entity<PdfView>> {
        match self.page {
            Page::PdfCalm => Some(&self.pdf[0]),
            Page::PdfSearch => Some(&self.pdf[1]),
            Page::PdfDrawers => Some(&self.pdf[2]),
            _ => None,
        }
    }

    pub fn focus_sample(&self, window: &mut Window, cx: &mut App) {
        if let Some(body) = self.sample() {
            window.focus(&body.focus_handle(cx), cx);
        }
        if let Some(pdf) = self.pdf_sample() {
            window.focus(&pdf.focus_handle(cx), cx);
        }
    }

    pub fn drive_sample(&self, window: &mut Window, cx: &mut App) {
        if self.page == Page::MouseMarks {
            let events = self.mouse.update(cx, |page, cx| page.take_drive(cx));
            for event in events {
                window.dispatch_event(event, cx);
            }
            return;
        }
        let steps: &[&str] = match self.page {
            Page::Tree => &TREE_STEPS,
            Page::Table => &TABLE_STEPS,
            Page::MemoryDrawers => &MEMORY_STEPS,
            Page::ElfDrawers => &ELF_STEPS,
            Page::TraceDrawers => &TRACE_STEPS,
            Page::MapDrawers => &MAP_STEPS,
            Page::PdfDrawers => &PDF_STEPS,
            _ => &[],
        };
        for name in steps {
            if let Ok(action) = cx.build_action(name, None) {
                window.dispatch_action(action, cx);
            }
        }
    }

    pub(super) fn sample_page(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let has_keys = matches!(
            self.page,
            Page::Tree
                | Page::MemoryImage
                | Page::MemoryDrawers
                | Page::ElfSymbols
                | Page::ElfDrawers
                | Page::TraceTable
                | Page::TraceDrawers
                | Page::MapTables
                | Page::MapDrawers
        );
        match self.sample() {
            Some(body) => root
                .child(body.view())
                .when(has_keys, |root| root.child(sample_keys(body, cx))),
            None => root.child("This sample did not load."),
        }
    }
}

pub(super) fn image_keys() -> FKeyBar {
    let kinds = [("Alt+1", "Text"), ("3", "Hex"), ("Esc", "Close")];
    FKeyBar::new("image-keys", ImageView::KEYS.into_iter().chain(kinds))
}

pub(super) fn sample_keys(body: &dyn ViewerBody, cx: &App) -> FKeyBar {
    let (BodyKeys::Modal(keys) | BodyKeys::Normal(keys)) = body.keys(cx);
    let viewer: &[(&'static str, &'static str)] = if body.takes_letters(cx) {
        &[("Alt+1", "Text"), ("Alt+3", "Hex"), ("Esc", "Close")]
    } else {
        &[
            ("1", "Text"),
            ("3", "Hex"),
            ("N", "Next file"),
            ("P", "Previous file"),
            ("Esc", "Close"),
        ]
    };
    let view = body.parts(cx).map(|_| ("Alt+V", "View"));
    FKeyBar::new(
        "sample-keys",
        keys.into_iter().chain(view).chain(viewer.iter().copied()),
    )
}
