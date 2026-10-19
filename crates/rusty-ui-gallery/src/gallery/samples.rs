use super::*;

impl Gallery {
    pub(super) fn sample_page(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let has_keys = matches!(
            self.page,
            Page::Tree
                | Page::MemoryImage
                | Page::MemoryDrawers
                | Page::ElfSymbols
                | Page::ElfDrawers
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
    FKeyBar::new(
        "sample-keys",
        keys.into_iter().chain(viewer.iter().copied()),
    )
}
