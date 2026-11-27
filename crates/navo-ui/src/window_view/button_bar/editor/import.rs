//! Icon import tab view: file picker, ExternalPaths drop zone, Keep colours toggle, and status alerts.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, ExternalPaths, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Role, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::{t as tr_args, tr};

use crate::file_list::row::inset_ring;
use crate::primitives::{Button, ButtonVariant, CheckBox, icon};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::{ButtonAction, ImportedIcon, ItemPath};

const DROP_ICON_SIZE: f32 = 24.;

struct ImportJob {
    generation: u64,
    selected: ItemPath,
    path: PathBuf,
}

/// Persistent toggle, busy flag, last SVG path, and status/error state for the Import tab.
pub(crate) struct ImportState {
    keep_colours: bool,
    busy: bool,
    last_svg: Option<PathBuf>,
    cleaned_removed: Option<Vec<String>>,
    error: Option<String>,
}

impl ImportState {
    /// Creates default import state with Keep colours off (`mono = true`).
    pub(crate) fn new() -> Self {
        Self {
            keep_colours: false,
            busy: false,
            last_svg: None,
            cleaned_removed: None,
            error: None,
        }
    }

    /// Clears per-item import status and last SVG path when selection changes.
    pub(crate) fn clear_status(&mut self) {
        self.busy = false;
        self.last_svg = None;
        self.cleaned_removed = None;
        self.error = None;
    }
}

impl NavoWindow {
    /// Renders the Import tab body with a drop zone, file chooser, Keep colours toggle, and notices.
    pub(super) fn import_body(
        &self,
        import: &ImportState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("button-bar-import")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.md)
            .child(self.import_drop_zone(import, t, cx))
            .child(self.import_keep_colours_row(import, t, cx))
            .when_some(
                import
                    .cleaned_removed
                    .as_ref()
                    .filter(|list| !list.is_empty()),
                |col, removed| col.child(cleaned_banner(removed, t)),
            )
            .when_some(import.error.as_deref(), |col, message| {
                col.child(error_banner(message, t))
            })
            .into_any_element()
    }

    fn import_drop_zone(
        &self,
        import: &ImportState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = t.colors.accent;
        let hover = t.colors.hover;
        let choose_label = if import.busy {
            tr("button-bar-import-choosing")
        } else {
            tr("button-bar-import-choose")
        };
        let choose = Button::new("button-bar-import-choose", choose_label)
            .variant(ButtonVariant::Primary)
            .disabled(import.busy)
            .on_click(cx.listener(|view, _, window, cx| view.choose_import_file(window, cx)));

        div()
            .id("button-bar-import-drop")
            .aria_label(tr("button-bar-import-drop-hint"))
            .test_support()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(t.spacing.sm)
            .p(t.spacing.xl)
            .rounded(t.radii.md)
            .border_1()
            .border_dashed()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.field)
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style
                    .border_color(accent)
                    .bg(hover)
                    .shadow(inset_ring(accent))
            })
            .on_drop(cx.listener(|view, paths: &ExternalPaths, window, cx| {
                view.import_dropped_paths(paths, window, cx);
            }))
            .child(icon(
                IconName::FileImage,
                px(DROP_ICON_SIZE),
                t.colors.muted,
            ))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(t.typography.small)
                    .text_color(t.colors.text)
                    .child(tr("button-bar-import-prompt")),
            )
            .child(
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(tr("button-bar-import-drop-hint")),
            )
            .child(choose)
            .child(
                div()
                    .text_size(t.typography.tiny)
                    .text_color(t.colors.muted)
                    .child(tr("button-bar-import-formats")),
            )
            .into_any_element()
    }

    fn import_keep_colours_row(
        &self,
        import: &ImportState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.xs)
            .child(
                CheckBox::new(
                    "button-bar-import-keep-colours",
                    tr("button-bar-import-keep-colours"),
                )
                .checked(import.keep_colours)
                .disabled(import.busy)
                .on_toggle(cx.listener(|view, _, window, cx| {
                    view.toggle_import_keep_colours(window, cx);
                })),
            )
            .child(
                div()
                    .text_size(t.typography.tiny)
                    .text_color(t.colors.muted)
                    .child(tr("button-bar-import-keep-colours-hint")),
            )
            .into_any_element()
    }

    fn choose_import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .button_bar
            .editor
            .as_ref()
            .is_none_or(|e| e.picker.import.busy)
        {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("button-bar-import-prompt").into()),
        });
        let generation = self.button_bar.import_generation;
        let selected = self.button_bar.editor.as_ref().and_then(|e| e.selected);
        cx.spawn_in(window, async move |this, cx| {
            let picked = match paths.await {
                Ok(Ok(Some(mut list))) => list.drain(..).next().map(Ok),
                Ok(Ok(None)) => None,
                Ok(Err(err)) => Some(Err(err.to_string())),
                Err(err) => Some(Err(err.to_string())),
            };
            let Some(outcome) = picked else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_choose_import(generation, selected, outcome, window, cx);
            });
        })
        .detach();
    }

    fn finish_choose_import(
        &mut self,
        generation: u64,
        selected: Option<ItemPath>,
        outcome: Result<PathBuf, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.import_generation != generation
            || self.button_bar.editor.as_ref().and_then(|e| e.selected) != selected
        {
            return;
        }
        match outcome {
            Ok(path) => self.start_import(path, window, cx),
            Err(err) => self.set_import_error(err, cx),
        }
    }

    fn import_dropped_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = paths.0.first().cloned() else {
            return;
        };
        self.start_import(path, window, cx);
    }

    fn toggle_import_keep_colours(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let reimport = {
            let Some(editor) = self.button_bar.editor.as_mut() else {
                return;
            };
            if editor.picker.import.busy {
                return;
            }
            editor.picker.import.keep_colours = !editor.picker.import.keep_colours;
            editor.picker.import.last_svg.clone()
        };
        if let Some(path) = reimport {
            self.start_import(path, window, cx);
        } else {
            cx.notify();
        }
    }

    fn start_import(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        if self
            .selected_item(Some(selected))
            .is_none_or(|item| matches!(item.action, ButtonAction::Separator))
        {
            return;
        }
        let Some(import_fn) = self.button_bar.hooks.as_ref().map(|h| h.import.clone()) else {
            return;
        };
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        if editor.picker.import.busy {
            return;
        }
        let mono = !editor.picker.import.keep_colours;
        editor.picker.import.busy = true;
        editor.picker.import.error = None;
        editor.picker.import.cleaned_removed = None;

        self.button_bar.import_generation = self.button_bar.import_generation.wrapping_add(1);
        let job = ImportJob {
            generation: self.button_bar.import_generation,
            selected,
            path: path.clone(),
        };

        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { import_fn(path, mono) })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_import(job, result, window, cx);
            });
        })
        .detach();
    }

    fn finish_import(
        &mut self,
        job: ImportJob,
        result: Result<ImportedIcon, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.import_generation != job.generation {
            return;
        }
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        if editor.selected != Some(job.selected) {
            return;
        }
        editor.picker.import.busy = false;
        match result {
            Ok(ImportedIcon { spec, removed }) => {
                editor.picker.import.error = None;
                editor.picker.import.cleaned_removed = (!removed.is_empty()).then_some(removed);
                editor.picker.import.last_svg = is_svg_path(&job.path).then_some(job.path);
                self.pick_editor_icon(spec, window, cx);
            }
            Err(message) => {
                editor.picker.import.last_svg = None;
                editor.picker.import.cleaned_removed = None;
                editor.picker.import.error = Some(message);
                cx.notify();
            }
        }
    }

    fn set_import_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.import.last_svg = None;
            editor.picker.import.cleaned_removed = None;
            editor.picker.import.error = Some(message);
            cx.notify();
        }
    }
}

fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

fn cleaned_banner(removed: &[String], t: &Theme) -> AnyElement {
    let items = removed.join(", ");
    let message = tr_args!("button-bar-import-cleaned", items = items);
    div()
        .id("button-bar-import-cleaned")
        .role(Role::Status)
        .aria_label(message.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .child(icon(IconName::Check, t.spacing.icon, t.colors.ok))
        .child(message)
        .into_any_element()
}

fn error_banner(message: &str, t: &Theme) -> AnyElement {
    div()
        .id("button-bar-import-error")
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.colors.mark)
        .bg(t.surfaces.field)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.mark))
        .child(message.to_owned())
        .into_any_element()
}
