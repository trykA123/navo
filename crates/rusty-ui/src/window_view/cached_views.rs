use std::collections::HashMap;

use gpui_kit::WeakEntity;
use rusty_core::PaneId;

use super::FerriteWindow;
use crate::prelude::*;
use crate::tokens;

#[derive(Clone, Copy)]
pub(super) enum Part {
    Title,
    Pane(PaneId),
}

pub(super) struct CachedPart {
    owner: WeakEntity<FerriteWindow>,
    part: Part,
}

pub(super) struct CachedViews {
    title: Entity<CachedPart>,
    panes: HashMap<PaneId, Entity<CachedPart>>,
    refresh_queued: bool,
}

fn cached(view: &Entity<CachedPart>, mut layout: Div) -> AnyElement {
    #[cfg(debug_assertions)]
    if std::env::var_os("NAVO_NO_VIEW_CACHE").is_some_and(|value| value == "1") {
        return view.clone().into_any_element();
    }
    view.clone()
        .cached(layout.style().clone())
        .into_any_element()
}

impl CachedViews {
    pub(super) fn new(cx: &mut Context<FerriteWindow>) -> Self {
        Self {
            title: Self::part(Part::Title, cx),
            panes: HashMap::new(),
            refresh_queued: false,
        }
    }

    fn part(part: Part, cx: &mut Context<FerriteWindow>) -> Entity<CachedPart> {
        let owner = cx.weak_entity();
        cx.new(|_| CachedPart { owner, part })
    }

    pub(super) fn title(&self, cx: &App) -> AnyElement {
        cached(
            &self.title,
            div()
                .h(tokens::current(cx).spacing.header_height)
                .flex_none(),
        )
    }
}

impl FerriteWindow {
    pub(super) fn cached_pane(&mut self, pane: PaneId, cx: &mut Context<Self>) -> AnyElement {
        let child = self
            .cached
            .panes
            .entry(pane)
            .or_insert_with(|| CachedViews::part(Part::Pane(pane), cx));
        cached(child, div().size_full())
    }

    pub(super) fn queue_cached_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.cached.refresh_queued, true) {
            return;
        }
        let owner = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = owner.update(cx, Self::refresh_cached);
        });
    }

    fn refresh_cached(&mut self, cx: &mut Context<Self>) {
        self.cached.refresh_queued = false;
        self.cached.title.update(cx, |_, cx| cx.notify());
        let panes = self.panes();
        self.cached.panes.retain(|pane, _| panes.contains(pane));
        for child in self.cached.panes.values() {
            child.update(cx, |_, cx| cx.notify());
        }
    }
}

impl Render for CachedPart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        super::tests::mouse_cost::note_child_render(self.part);
        let part = self.part;
        self.owner
            .update(cx, |owner, cx| match part {
                Part::Title => owner.title_bar(cx).into_any_element(),
                Part::Pane(pane) => owner.pane_view(pane, window, cx),
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}
