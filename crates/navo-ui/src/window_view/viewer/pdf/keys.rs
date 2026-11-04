use super::{PDF_CONTEXT, PdfView, Phase};
use crate::prelude::*;
use crate::window_view::viewer::body::{BodyKeys, Key};
use crate::window_view::viewer::keys_bar::{hints, pair};

gpui_kit::actions!(
    viewer_pdf,
    [
        FitWidth,
        ActualSize,
        ZoomIn,
        ZoomOut,
        WholeWord,
        AllHits,
        PagesDrawer,
        CopyText,
        LinkNext,
        LinkPrevious,
        LinkFollow,
        TrailBack,
        TrailForward,
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "viewer_pdf");
}

const VIEWER: &str = "Viewer";
const FIND_ALL: &str = "FindAll";

fn reading(cx: &App) -> Vec<Key> {
    hints(
        cx,
        &[
            ("find.open", VIEWER, "Find"),
            ("viewer.go_to_line", VIEWER, "Page"),
            ("viewer.page_down", VIEWER, "Next page"),
            ("pdf.fit", PDF_CONTEXT, "Fit"),
            ("view.left_drawer", VIEWER, "Pages"),
            ("view.right_drawer", VIEWER, "All hits"),
        ],
    )
}

fn finding(cx: &App) -> Vec<Key> {
    let mut keys = vec![("Enter", "Next"), ("Shift+Enter", "Previous")];
    keys.extend(hints(
        cx,
        &[
            ("find.match_case", VIEWER, "Case"),
            ("pdf.whole_word", PDF_CONTEXT, "Word"),
            ("pdf.all_hits", PDF_CONTEXT, "All hits"),
            ("viewer.back", VIEWER, "Close find"),
        ],
    ));
    keys
}

fn drawer(cx: &App) -> Vec<Key> {
    let mut keys = Vec::from_iter(pair(
        cx,
        ("find.result_up", "find.result_down"),
        FIND_ALL,
        "Move",
    ));
    keys.extend(hints(
        cx,
        &[
            ("find.go_to_result", FIND_ALL, "Go"),
            ("find.close_results", FIND_ALL, "Close drawer"),
        ],
    ));
    keys
}

fn locked(cx: &App) -> Vec<Key> {
    let mut keys = vec![("Enter", "Open")];
    keys.extend(hints(
        cx,
        &[
            ("viewer.show_hex", VIEWER, "Hex"),
            ("viewer.back", VIEWER, "Cancel"),
        ],
    ));
    keys
}

impl PdfView {
    pub(super) fn keys(&self, cx: &App) -> BodyKeys {
        match &self.phase {
            Phase::Locked { .. } | Phase::Unlocking { .. } => BodyKeys::Modal(locked(cx)),
            Phase::Refused(_) => BodyKeys::Normal(Vec::new()),
            Phase::Ready(_) if !self.drawers.is_empty() => BodyKeys::Modal(drawer(cx)),
            Phase::Ready(_) if self.is_finding => BodyKeys::Modal(finding(cx)),
            Phase::Ready(_) => BodyKeys::Normal(reading(cx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_view::viewer::keys_bar::rebind;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn reading_and_finding_hints_show_user_keys(cx: &mut TestAppContext) {
        rebind(cx, "find.open", "\"alt-f\"");
        assert!(cx.update(|cx| reading(cx)).contains(&("Alt+F", "Find")));
        rebind(cx, "pdf.whole_word", "\"alt-j\"");
        assert!(cx.update(|cx| finding(cx)).contains(&("Alt+J", "Word")));
    }
}
