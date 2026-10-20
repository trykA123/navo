use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::follow::Target;
use super::marks::Marks;
use super::spot::Carry;
use super::{Preview, Shown};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

const SEPARATOR: &str = " › ";

pub(super) struct Crumb {
    target: Target,
    carry: Option<Carry>,
    pub marks: Marks,
}

impl Preview {
    pub(super) fn crumbs(&self) -> Vec<String> {
        if self.trail.is_empty() {
            return Vec::new();
        }
        let current = match &self.shown {
            Some(Shown::File(file)) => Some(file.target.name()),
            Some(Shown::Folder(folder)) => Some(folder.target.name()),
            Some(Shown::Empty) | None => None,
        };
        self.trail
            .iter()
            .map(|crumb| crumb.target.name())
            .chain(current)
            .collect()
    }
}

impl FerriteWindow {
    pub(super) fn preview_into(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = self.preview.folder() else {
            return;
        };
        let Some(target) = folder.spot_target() else {
            return;
        };
        let crumb = Crumb {
            target: folder.target.clone(),
            carry: folder.carry(),
            marks: Marks::default(),
        };
        let is_folder = target.is_folder();
        self.preview.trail.push(crumb);
        if !is_folder {
            self.keep_marks();
        }
        let shown = if target.is_folder() {
            let mut folder = self.start_folder(target, None);
            folder.want_spot();
            Shown::Folder(Box::new(folder))
        } else {
            Shown::File(Box::new(self.start_file(target, window, cx)))
        };
        self.show_nested(shown, window, cx);
    }

    pub(super) fn preview_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(crumb) = self.preview.trail.pop() else {
            return;
        };
        let mut folder = self.start_folder(crumb.target, None);
        match crumb.carry {
            Some(Carry::Largest(_)) if folder.hides_largest => folder.want_spot(),
            Some(carry) => folder.adopt(carry),
            None => folder.want_spot(),
        }
        folder.marks = crumb.marks;
        self.show_nested(Shown::Folder(Box::new(folder)), window, cx);
    }

    pub(super) fn nested_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.trail.is_empty() {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.preview_up(window, cx);
    }

    fn keep_marks(&mut self) {
        let marks = self
            .preview
            .folder_mut()
            .map(|f| std::mem::take(&mut f.marks));
        if let (Some(marks), Some(crumb)) = (marks, self.preview.trail.last_mut()) {
            crumb.marks = marks;
        }
    }

    fn show_nested(&mut self, shown: Shown, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.stop_loading();
        self.preview.is_stale = false;
        self.preview.loads += 1;
        self.preview.shown = Some(shown);
        self.apply_preview_parts();
        let target = self.preview.browse_target(cx);
        window.focus(&target, cx);
        cx.notify();
    }
}

pub(super) fn breadcrumb(t: &Theme, crumbs: &[String]) -> Option<AnyElement> {
    let (last, before) = crumbs.split_last()?;
    let parts = before.iter().flat_map(|name| {
        [
            div()
                .flex_shrink(1.)
                .min_w_0()
                .truncate()
                .text_color(t.colors.muted)
                .child(name.clone())
                .into_any_element(),
            icon(
                IconName::ChevronRight,
                t.spacing.icon - px(4.),
                t.colors.faint,
            )
            .into_any_element(),
        ]
    });
    Some(
        div()
            .id("preview-breadcrumb")
            .role(Role::Navigation)
            .aria_label(crumbs.join(SEPARATOR))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .px(t.spacing.xxl)
            .pt(t.spacing.md)
            .text_size(t.typography.small)
            .children(parts)
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(last.clone()),
            )
            .into_any_element(),
    )
}
