use gpui_kit::ModifiersChangedEvent;

use super::{FerriteWindow, input_trace, keys};
use crate::app_menu::ToggleAppMenu;
use crate::prelude::*;
use crate::settings::ToggleSettings;
use crate::title_bar::{GradientLayer, system_backdrop};
use crate::tokens;

impl Render for FerriteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        super::tests::mouse_cost::note_render();
        let trace = input_trace::frame(window, cx);
        self.sync_workspaces(window, cx);
        let frame = self.perf_frame(window);
        self.watch_start(window, cx);
        let below_title_bar = self.below_title_bar(window, cx);
        let root = root_surface(cx)
            .children(trace)
            .child(self.cached.title(cx))
            .child(below_title_bar)
            .on_modifiers_changed(
                cx.listener(|view, event: &ModifiersChangedEvent, window, cx| {
                    view.track_modifiers(event.modifiers, window, cx);
                }),
            )
            .on_action(cx.listener(|view, _: &ToggleAppMenu, window, cx| {
                view.toggle_app_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ToggleSettings, window, cx| {
                view.toggle_settings(window, cx);
            }))
            .on_action(cx.listener(|view, _: &keys::ToggleJobs, _, cx| view.jobs_key(cx)))
            .children(Self::noted_modal(self.modal_view(window, cx)))
            .children(self.app_menu_view(cx))
            .children(self.shell_menu_view())
            .children(self.viewer_menu_view())
            .children(self.view_menus());
        let root = self.drag_layer(Self::perf_on(root, frame, cx), cx);
        let root = Self::dialog_input_on(root, cx);
        let root = self.pane_swap_layer(root, cx);
        let root = self.tab_drag_layer(root, window, cx);
        let root = Self::open_window_menu_on(Self::multi_rename_on(root, cx), cx);
        let root = Self::help_on(Self::layout_switcher_on(root, cx), cx);
        let root = self.command_center_on(self.activity_keys(self.crash_keys(root, cx), cx), cx);
        self.preview_keys(root, window, cx)
    }
}

fn root_surface(cx: &App) -> Div {
    let t = tokens::current(cx);
    let gradient = system_backdrop().gradient;
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .when(!t.is_contrast && gradient == GradientLayer::Root, |root| {
            root.bg(t.surfaces.mica())
        })
        .when(t.is_contrast, |root| root.bg(t.colors.background))
        .text_color(t.colors.text)
        .font_family(t.typography.ui_font.clone())
        .text_size(t.typography.body)
        .line_height(t.typography.line_height)
}
