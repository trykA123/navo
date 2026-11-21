//! Check of the Folders row layout: every line fills the list, so the cells share columns.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use sidewise_ui::browser::{CompareKind, CompareRow, CompareSide, RowStatus};

use super::tests::view;

/// A file side with a placeholder size.
fn file() -> Option<CompareSide> {
    Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    })
}

/// Names of different lengths, one left-only and one right-only: all lines must be equally wide
/// and the right cell must start at the same x on each.
#[gpui_kit::test]
fn lines_fill_the_list_and_cells_line_up(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    let rows = vec![
        CompareRow::new(1, "a.txt", RowStatus::Different, (file(), file())),
        CompareRow::new(
            2,
            "a-much-longer-name.txt",
            RowStatus::OnlyLeft,
            (file(), None),
        ),
        CompareRow::new(3, "mid.txt", RowStatus::OnlyRight, (None, file())),
    ];
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |this, cx| this.set_rows(rows, cx));
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
    let (lines, rights) = cx
        .update_window(window, |_, w, _| {
            let of = |id: &'static str| {
                (0..3usize)
                    .map(|i| w.find((id, i)).bounds())
                    .collect::<Vec<_>>()
            };
            (of("folder-line"), of("cell-r"))
        })
        .expect("window");
    let width = lines[0].size.width;
    assert!(lines.iter().all(|b| b.size.width == width), "{lines:?}");
    assert!(
        rights.iter().all(|b| b.origin.x == rights[0].origin.x),
        "{rights:?}"
    );
    assert!(
        f32::from(width) > 300.,
        "line is as wide as the list: {width:?}"
    );
}
