use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use rusty_core::{AppState, Command, CursorMove};
use rusty_fs::FsEntry;
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::prelude::*;
use rusty_ui::tokens::{self, Theme};

use crate::pane_page::{location, run_on};
use crate::pane_sample;

const CAPTIONS: [&str; 3] = [
    "Ctrl+click rows 2 and 4, then Shift+click row 7",
    "Right-drag from row 3 down to row 7",
    "Lasso from below the last row up to row 6, row 2 marked before",
];

type Slot = Rc<Cell<Option<Bounds<Pixels>>>>;

struct Panel {
    state: AppState,
    bounds: Slot,
}

pub struct MousePage {
    rows: Arc<[FsEntry]>,
    panels: [Panel; 3],
    is_driven: bool,
}

impl MousePage {
    pub fn new(_: &mut Context<Self>) -> Self {
        let rows = pane_sample::rows();
        let panels = [(); 3].map(|()| {
            let mut state = AppState::new(location(r"D:\work\commander"));
            run_on(&mut state, &rows, &Command::PaneRefresh);
            Panel {
                state,
                bounds: Slot::default(),
            }
        });
        let mut page = Self {
            rows,
            panels,
            is_driven: false,
        };
        let to = CursorMove::Row(1);
        page.run(2, &Command::PaneCursorMove { to });
        page.run(2, &Command::SelectionToggle);
        page
    }

    fn run(&mut self, index: usize, command: &Command) {
        if let Some(panel) = self.panels.get_mut(index) {
            run_on(&mut panel.state, &self.rows, command);
        }
    }

    fn row_point(&self, (index, row): (usize, f32), t: &Theme) -> Option<Point<Pixels>> {
        let bounds = self.panels.get(index)?.bounds.get()?;
        let y = bounds.top() + t.spacing.row_height + t.rows.height * (row + 0.5);
        Some(point(bounds.left() + bounds.size.width * 0.5, y))
    }

    pub fn take_drive(&mut self, cx: &App) -> Vec<PlatformInput> {
        if std::mem::replace(&mut self.is_driven, true) {
            return Vec::new();
        }
        let t = tokens::current(cx).clone();
        let at = |spot| self.row_point(spot, &t);
        let (Some(clicks), Some(drag), Some(lasso)) = (
            [1., 3., 6.]
                .map(|row| at((0, row)))
                .into_iter()
                .collect::<Option<Vec<_>>>(),
            [2., 3., 4., 5., 6.]
                .map(|row| at((1, row)))
                .into_iter()
                .collect::<Option<Vec<_>>>(),
            at((2, self.rows.len() as f32 + 1.5)).zip(at((2, 5.))),
        ) else {
            return Vec::new();
        };
        let mut events = click_events(&clicks);
        events.extend(drag_events(MouseButton::Right, &drag, true));
        let (from, to) = lasso;
        let to = point(to.x - px(120.), to.y);
        events.extend(drag_events(MouseButton::Left, &[from, to], false));
        events
    }

    fn panel(&self, index: usize, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let panel = self.panels.get(index)?;
        let tab = panel.state.active_tab();
        let props = FileListProps {
            folder: "commander".into(),
            rows: self.rows.clone(),
            view: tab.view(),
            marked: Rc::new(tab.selection().clone()),
            is_active: true,
            filter: None,
            sort: SortKey::default(),
        };
        let list = FileList::new(("mouse-list", index), props).on_command(cx.listener(
            move |this: &mut Self, command: &Command, _, cx| {
                this.run(index, command);
                cx.notify();
            },
        ));
        let slot = panel.bounds.clone();
        let frame = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .rounded(t.radii.md)
            .overflow_hidden()
            .on_children_prepainted(move |bounds, _, _| slot.set(bounds.first().copied()))
            .child(list);
        let caption = div()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(CAPTIONS.get(index).copied().unwrap_or_default());
        Some(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(t.spacing.sm)
                .child(caption)
                .child(frame),
        )
    }
}

fn click_events(points: &[Point<Pixels>]) -> Vec<PlatformInput> {
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let held = [ctrl, ctrl, shift];
    points
        .iter()
        .zip(held)
        .flat_map(|(position, modifiers)| {
            let down = MouseDownEvent {
                button: MouseButton::Left,
                position: *position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            };
            let up = MouseUpEvent {
                button: MouseButton::Left,
                position: *position,
                modifiers,
                click_count: 1,
            };
            [down.to_platform_input(), up.to_platform_input()]
        })
        .collect()
}

fn drag_events(button: MouseButton, path: &[Point<Pixels>], releases: bool) -> Vec<PlatformInput> {
    let [from, rest @ ..] = path else {
        return Vec::new();
    };
    let modifiers = Modifiers::default();
    let down = MouseDownEvent {
        button,
        position: *from,
        modifiers,
        click_count: 1,
        first_mouse: false,
    };
    let moves = rest.iter().map(|position| {
        MouseMoveEvent {
            position: *position,
            pressed_button: Some(button),
            modifiers,
        }
        .to_platform_input()
    });
    let last = rest.last().copied().unwrap_or(*from);
    let up = MouseUpEvent {
        button,
        position: last,
        modifiers,
        click_count: 1,
    };
    std::iter::once(down.to_platform_input())
        .chain(moves)
        .chain(releases.then(|| up.to_platform_input()))
        .collect()
}

impl Render for MousePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let panels: Vec<_> = (0..3)
            .filter_map(|index| self.panel(index, &t, cx))
            .collect();
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.xl)
            .p(t.spacing.xl)
            .bg(t.colors.surface)
            .children(panels)
    }
}
