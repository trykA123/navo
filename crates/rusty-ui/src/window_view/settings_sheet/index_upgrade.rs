use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_l10n::tr;

use crate::pane_chrome::card_shadow;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens;
use crate::window_view::FerriteWindow;

impl FerriteWindow {
    pub(in crate::window_view) fn check_index_upgrade(&mut self, cx: &mut Context<Self>) {
        let due = self.settings.search.service.upgrade_due.clone();
        cx.spawn(async move |this, cx| {
            let is_due = cx.background_spawn(async move { due() }).await;
            let _ = this.update(cx, |view, cx| {
                view.settings.index_upgrade_due = is_due;
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::window_view) fn index_upgrade_view(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.settings.index_upgrade_due {
            return None;
        }
        let t = tokens::current(cx).clone();
        let (run, dismiss) = (cx.weak_entity(), cx.weak_entity());
        let title = tr("index-upgrade-title");
        let upgrade = Button::new("index-upgrade-run", tr("index-upgrade-run"))
            .variant(ButtonVariant::Primary)
            .on_click(move |_, _, cx| {
                let _ = run.update(cx, |view, cx| view.run_index_upgrade(cx));
            });
        let later = Button::new("index-upgrade-dismiss", tr("index-upgrade-dismiss"))
            .variant(ButtonVariant::Ghost)
            .on_click(move |_, _, cx| {
                let _ = dismiss.update(cx, |view, cx| view.dismiss_index_upgrade(cx));
            });
        let card = div()
            .id("index-upgrade")
            .role(Role::Status)
            .aria_label(title.clone())
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .px(t.spacing.lg)
            .py(t.spacing.md)
            .rounded(t.radii.card)
            .bg(t.surfaces.card)
            .shadow(card_shadow(&t, false))
            .child(icon(IconName::Database, t.spacing.icon, t.colors.accent))
            .child(notice_text(&t, title))
            .child(upgrade)
            .child(later);
        Some(
            div()
                .flex_none()
                .mx(t.spacing.xs * 2.)
                .my(t.spacing.xs)
                .child(card)
                .into_any_element(),
        )
    }

    fn run_index_upgrade(&mut self, cx: &mut Context<Self>) {
        if !self.settings.index_upgrade_due {
            return;
        }
        self.settings.index_upgrade_due = false;
        let install = self.settings.search.service.install.clone();
        self.load_index_service(Some(install), cx);
        cx.notify();
    }

    fn dismiss_index_upgrade(&mut self, cx: &mut Context<Self>) {
        if !self.settings.index_upgrade_due {
            return;
        }
        self.settings.index_upgrade_due = false;
        let dismiss = self.settings.search.service.dismiss_upgrade.clone();
        cx.background_spawn(async move { dismiss() }).detach();
        cx.notify();
    }
}

fn notice_text(t: &tokens::Theme, title: String) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.xxs)
        .child(div().font_weight(FontWeight::MEDIUM).child(title))
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(tr("index-upgrade-body")),
        )
}
