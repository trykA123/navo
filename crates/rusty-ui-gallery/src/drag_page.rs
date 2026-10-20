use rusty_core::DropOp;
use rusty_ui::drag::{DragBadge, DragState, Hover, Refusal};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

pub struct DragPage {
    badges: Vec<Entity<DragBadge>>,
}

fn hover(op: Option<DropOp>, refusal: Option<Refusal>) -> Option<Hover> {
    Some(Hover {
        target_label: "Projects".into(),
        op,
        refusal: refusal.map(Refusal::text),
    })
}

fn state(count: usize, hover: Option<Hover>, keys_line: bool) -> DragState {
    DragState {
        count,
        first_name: "report.pdf".into(),
        hover,
        keys_line,
    }
}

impl DragPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let states = [
            state(1, None, true),
            state(3, hover(Some(DropOp::Move), None), true),
            state(1, hover(Some(DropOp::Copy), None), true),
            state(3, hover(Some(DropOp::Link), None), false),
            state(2, hover(None, Some(Refusal::SameFolder)), true),
            state(2, hover(None, Some(Refusal::IntoItself)), true),
        ];
        let badges = states
            .into_iter()
            .map(|state| {
                let state = cx.new(|_| state);
                cx.new(|cx| DragBadge::new(state, cx))
            })
            .collect();
        Self { badges }
    }
}

impl Render for DragPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .items_start()
            .gap(t.spacing.lg)
            .p(t.spacing.xl)
            .bg(t.colors.surface)
            .children(self.badges.iter().cloned())
    }
}
