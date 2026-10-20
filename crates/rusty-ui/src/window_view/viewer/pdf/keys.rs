use super::{PdfView, Phase};
use crate::prelude::*;
use crate::window_view::viewer::body::{BodyKeys, Key};

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

const READING: [Key; 6] = [
    ("Ctrl+F", "Find"),
    ("Ctrl+G", "Page"),
    ("PgDn", "Next page"),
    ("Ctrl+0", "Fit"),
    ("Ctrl+B", "Pages"),
    ("Ctrl+I", "All hits"),
];
const FINDING: [Key; 6] = [
    ("Enter", "Next"),
    ("Shift+Enter", "Previous"),
    ("Alt+C", "Case"),
    ("Alt+W", "Word"),
    ("Ctrl+Shift+F", "All hits"),
    ("Esc", "Close find"),
];
const DRAWER: [Key; 3] = [
    ("Up/Down", "Move"),
    ("Enter", "Go"),
    ("Esc", "Close drawer"),
];
const LOCKED: [Key; 3] = [("Enter", "Open"), ("Alt+3", "Hex"), ("Esc", "Cancel")];

impl PdfView {
    pub(super) fn keys(&self) -> BodyKeys {
        match &self.phase {
            Phase::Locked { .. } | Phase::Unlocking { .. } => BodyKeys::Modal(LOCKED.to_vec()),
            Phase::Refused(_) => BodyKeys::Normal(Vec::new()),
            Phase::Ready(_) if !self.drawers.is_empty() => BodyKeys::Modal(DRAWER.to_vec()),
            Phase::Ready(_) if self.is_finding => BodyKeys::Modal(FINDING.to_vec()),
            Phase::Ready(_) => BodyKeys::Normal(READING.to_vec()),
        }
    }
}
