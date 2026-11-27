//! Icon picker pane: background tag index loading, set filters, brand colour toggle, and licences view.

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Div, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, Stateful, StatefulInteractiveElement, Styled, Subscription, Window, div,
};
use navo_icons::registry::{NOTICE_TXT, registry};
use navo_icons::search::TagIndex;
use navo_icons::spec::IconSpec;
use navo_l10n::tr;

use super::editor_tab_button;
use super::picker_grid::{PickerRow, build_rows, picker_virtual_list};
use crate::primitives::{
    Button, ButtonVariant, CheckBox, ListScroll, TextField, TextFieldState, text_field_state,
};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::ButtonAction;
use crate::window_view::button_bar::state::ButtonBarEditorState;

/// Active icon-set filter chip in the icon picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PickerFilter {
    #[default]
    All,
    Lucide,
    Tabler,
    Phosphor,
    Brands,
    Downloaded,
}

/// Persistent search query, filter, cached grid rows, and background tag index for the picker.
pub(crate) struct PickerState {
    search_input: gpui_kit::Entity<TextFieldState>,
    filter: PickerFilter,
    brand_colour: bool,
    show_licences: bool,
    index: Option<TagIndex>,
    loading: bool,
    scroll: ListScroll,
    rows: Rc<[PickerRow]>,
    _sub: Subscription,
}

impl PickerState {
    /// Creates an empty picker state in loading mode and subscribes to query changes.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<NavoWindow>) -> Self {
        let search_input = text_field_state(tr("button-bar-picker-search"), window, cx);
        let sub = cx.subscribe(&search_input, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.picker_query_changed(cx);
            }
        });
        Self {
            search_input,
            filter: PickerFilter::default(),
            brand_colour: false,
            show_licences: false,
            index: None,
            loading: true,
            scroll: ListScroll::new(),
            rows: Rc::from([]),
            _sub: sub,
        }
    }

    /// Returns whether the background `TagIndex` has finished loading.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "inspected by picker lifecycle tests")
    )]
    pub(crate) fn has_index(&self) -> bool {
        self.index.is_some()
    }

    fn refresh_rows(&mut self, cx: &App) {
        self.scroll.scroll_to_top(0);
        let Some(index) = self.index.as_ref() else {
            self.rows = Rc::from([]);
            return;
        };
        let query = self.search_input.read(cx).value();
        self.rows = build_rows(index, &query, self.filter);
    }

    fn has_brands_group(&self) -> bool {
        self.rows
            .iter()
            .any(|row| matches!(row, PickerRow::Header { set_id, .. } if set_id == "brands"))
    }
}

impl Drop for PickerState {
    fn drop(&mut self) {
        self.index = None;
        registry().trim();
    }
}

impl NavoWindow {
    /// Spawns background loading of `TagIndex`, guarded by `picker_generation` and `WeakEntity`.
    pub(super) fn start_picker_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.button_bar.picker_generation = self.button_bar.picker_generation.wrapping_add(1);
        let generation = self.button_bar.picker_generation;
        cx.spawn_in(window, async move |this, cx| {
            let index = cx
                .background_spawn(async { TagIndex::load(registry()) })
                .await;
            let _ = this.update_in(cx, |view, _, cx| {
                view.finish_picker_load(generation, index, cx);
            });
        })
        .detach();
    }

    fn finish_picker_load(&mut self, generation: u64, index: TagIndex, cx: &mut Context<Self>) {
        if self.button_bar.picker_generation != generation {
            return;
        }
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        editor.picker.index = Some(index);
        editor.picker.loading = false;
        editor.picker.refresh_rows(cx);
        cx.notify();
    }

    /// Renders the icon picker pane (or a hint when no editable item is selected).
    pub(super) fn editor_icon_pane(
        &self,
        editor: &ButtonBarEditorState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let pane = div()
            .id("button-bar-editor-icon-pane")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .p(t.spacing.xl);

        let Some(item) = self.selected_item(editor.selected) else {
            return pane.child(muted_hint(tr("button-bar-editor-select-hint"), t));
        };
        if matches!(item.action, ButtonAction::Separator) {
            return pane.child(muted_hint(tr("button-bar-picker-separator-hint"), t));
        }
        if editor.picker.show_licences {
            return pane.child(self.licences_view(t, cx));
        }
        pane.child(self.picker_body(&editor.picker, t, cx))
    }

    fn picker_body(&self, picker: &PickerState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let show_brand_toggle = picker.filter == PickerFilter::Brands || picker.has_brands_group();
        let grid = self.picker_grid_area(picker, t, cx);

        div()
            .id("button-bar-picker")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(TextField::new(
                "button-bar-picker-search",
                &picker.search_input,
                tr("button-bar-picker-search"),
            ))
            .child(self.picker_filter_chips(picker.filter, t, cx))
            .when(show_brand_toggle, |col| {
                col.child(
                    div().flex().flex_none().items_center().child(
                        CheckBox::new(
                            "button-bar-picker-brand-colour",
                            tr("button-bar-picker-brand-colour"),
                        )
                        .checked(picker.brand_colour)
                        .on_toggle(cx.listener(|view, _, _, cx| {
                            view.toggle_picker_brand_colour(cx);
                        })),
                    ),
                )
            })
            .child(grid)
            .child(self.picker_footer(t, cx))
            .into_any_element()
    }

    fn picker_filter_chips(
        &self,
        active: PickerFilter,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips = [
            (
                PickerFilter::All,
                "button-bar-picker-filter-all",
                "button-bar-picker-filter-all",
            ),
            (
                PickerFilter::Lucide,
                "button-bar-picker-filter-lucide",
                "button-bar-picker-filter-lucide",
            ),
            (
                PickerFilter::Tabler,
                "button-bar-picker-filter-tabler",
                "button-bar-picker-filter-tabler",
            ),
            (
                PickerFilter::Phosphor,
                "button-bar-picker-filter-phosphor",
                "button-bar-picker-filter-phosphor",
            ),
            (
                PickerFilter::Brands,
                "button-bar-picker-filter-brands",
                "button-bar-picker-filter-brands",
            ),
            (
                PickerFilter::Downloaded,
                "button-bar-picker-filter-downloaded",
                "button-bar-picker-filter-downloaded",
            ),
        ];

        let mut row = div()
            .id("button-bar-picker-filters")
            .role(Role::TabList)
            .aria_label(tr("button-bar-picker-filters-label"))
            .test_support()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.xs);

        for (filter, id, msg) in chips {
            let chip = editor_tab_button(id, tr(msg), active == filter, t).on_click(cx.listener(
                move |view, _, _, cx| {
                    view.set_picker_filter(filter, cx);
                },
            ));
            row = row.child(chip);
        }
        row.into_any_element()
    }

    fn picker_grid_area(
        &self,
        picker: &PickerState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if picker.loading {
            return div()
                .flex_1()
                .min_h_0()
                .child(muted_hint(tr("button-bar-picker-loading"), t))
                .into_any_element();
        }
        if picker.rows.is_empty() {
            let empty_msg = if picker.filter == PickerFilter::Downloaded && !has_downloaded_sets() {
                tr("button-bar-picker-no-downloaded")
            } else {
                tr("button-bar-picker-no-results")
            };
            return div()
                .flex_1()
                .min_h_0()
                .child(muted_hint(empty_msg, t))
                .into_any_element();
        }
        picker_virtual_list(
            Rc::clone(&picker.rows),
            &picker.scroll,
            picker.brand_colour,
            cx.entity(),
            t,
        )
    }

    fn picker_footer(&self, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("button-bar-picker-footer")
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .pt(t.spacing.xs)
            .child(
                Button::new(
                    "button-bar-picker-licences",
                    tr("button-bar-picker-licences"),
                )
                .variant(ButtonVariant::Ghost)
                .on_click(cx.listener(|view, _, _, cx| {
                    view.set_picker_show_licences(true, cx);
                })),
            )
    }

    fn licences_view(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let back = Button::new(
            "button-bar-picker-licences-back",
            tr("button-bar-picker-licences-back"),
        )
        .variant(ButtonVariant::Secondary)
        .on_click(cx.listener(|view, _, _, cx| {
            view.set_picker_show_licences(false, cx);
        }));

        let lines = NOTICE_TXT.lines().enumerate().map(|(idx, line)| {
            let text = if line.is_empty() {
                String::from(" ")
            } else {
                line.to_owned()
            };
            div().id(("button-bar-picker-notice-line", idx)).child(text)
        });

        div()
            .id("button-bar-picker-licences-view")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.md)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(t.typography.small)
                            .text_color(t.colors.text)
                            .child(tr("button-bar-picker-licences-title")),
                    )
                    .child(back),
            )
            .child(
                div()
                    .id("button-bar-picker-licences-text")
                    .role(Role::Region)
                    .aria_label(NOTICE_TXT)
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .rounded(t.radii.md)
                    .border_1()
                    .border_color(t.surfaces.card_edge)
                    .bg(t.surfaces.field)
                    .p(t.spacing.md)
                    .gap(t.spacing.xs)
                    .text_size(t.typography.small)
                    .text_color(t.colors.text)
                    .children(lines),
            )
            .into_any_element()
    }

    fn picker_query_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.refresh_rows(cx);
            cx.notify();
        }
    }

    fn set_picker_filter(&mut self, filter: PickerFilter, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut()
            && editor.picker.filter != filter
        {
            editor.picker.filter = filter;
            editor.picker.refresh_rows(cx);
            cx.notify();
        }
    }

    fn toggle_picker_brand_colour(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.brand_colour = !editor.picker.brand_colour;
            cx.notify();
        }
    }

    fn set_picker_show_licences(&mut self, show: bool, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.show_licences = show;
            cx.notify();
        }
    }

    pub(super) fn pick_editor_icon(
        &mut self,
        spec: IconSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        if self
            .selected_item(Some(selected))
            .is_none_or(|item| matches!(item.action, ButtonAction::Separator))
        {
            return;
        }
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.picked_icon = Some(spec);
            editor.form.error = None;
            editor.form.debounce = None;
        }
        self.submit_form_update(window, cx);
    }
}

fn has_downloaded_sets() -> bool {
    let reg = registry();
    reg.sets().iter().any(|info| reg.is_downloaded(&info.id))
}

fn muted_hint(text: String, t: &Theme) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}
