mod compute;
mod gather;
mod inputs;
mod keys;
mod menu;
mod names;
mod names_file;
mod names_view;
mod ordering;
mod presets;
mod run;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FocusHandle, Focusable, Subscription, Task};
use rusty_core::Location;
use rusty_core::rename::check::RowState;
use rusty_l10n::tr;

use super::FerriteWindow;
use crate::multi_rename::view::fields::{FieldErrors, FieldInputs, FieldsEvent};
use crate::multi_rename::{Form, Preview, PreviewEvent, PreviewView, Shown};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

#[derive(Default)]
pub(super) struct MultiRenameState {
    open: Option<Open>,
}

struct Open {
    form: Form,
    source: compute::Source,
    title: SharedString,
    inputs: FieldInputs,
    view: Entity<PreviewView>,
    preview: Option<Preview>,
    errors: FieldErrors,
    generation: u64,
    run_waiting_for: Option<Form>,
    menu: Option<menu::Menu>,
    names: Option<names::Editor>,
    sorted: Option<(usize, bool)>,
    root: FocusHandle,
    preview_focus: FocusHandle,
    last_field: FocusHandle,
    compute: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Open {
    fn is_current(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|preview| preview.generation == self.generation)
    }

    fn push_preview(&self, cx: &mut App) {
        let Some(preview) = &self.preview else {
            return;
        };
        let blocked;
        let shown = if self.errors == FieldErrors::default()
            && self.form.names_error(self.source.items.len()).is_none()
        {
            preview
        } else {
            blocked = Preview {
                plan: None,
                ..preview.clone()
            };
            &blocked
        };
        let items = Arc::clone(&self.source.items);
        let compiled = self.form.compile().ok();
        let (fold, problems_only) = (self.source.fold, self.form.problems_only);
        self.view.update(cx, |view, cx| {
            view.set_sort(self.sorted);
            view.set_preview(
                Shown {
                    items: &items,
                    preview: shown,
                    fold,
                    problems_only,
                    matcher: compiled.as_ref(),
                    edited: self.form.edited_names().is_some(),
                },
                cx,
            );
        });
    }
}

fn first_new_name(preview: &Preview) -> Option<String> {
    preview
        .rows
        .iter()
        .find(|row| matches!(row.state, RowState::Changes { .. }))
        .map(|row| row.new.clone())
}

fn folder_title(folder: &Location, count: usize) -> SharedString {
    let name = folder.name().map(|name| name.to_text()).unwrap_or_default();
    crate::multi_rename::rename_title(count, &name).into()
}

impl FerriteWindow {
    pub(super) fn rename_row(&mut self, row: u64, window: &mut Window, cx: &mut Context<Self>) {
        let marks = self.browser.state().active_tab().selection().marked().len();
        if marks >= 2 {
            self.open_multi_rename(window, cx);
        } else {
            self.ask_rename(row);
        }
    }

    pub(super) fn open_multi_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.multi_rename.open.is_some()
            || self.prompt.is_some()
            || self.browser.pending().is_some()
        {
            return;
        }
        let batch = match gather::gather(&self.browser) {
            gather::Gathered::Ready(batch) => batch,
            gather::Gathered::Loading => {
                self.show_notice(StatusNotice {
                    text: tr("rename-open-loading").into(),
                    is_error: true,
                });
                cx.notify();
                return;
            }
            gather::Gathered::Empty => return,
        };
        let open = self.build_open(batch, window, cx);
        let first = open.inputs.name_mask.clone();
        self.browser.dialog_opened();
        self.multi_rename.open = Some(open);
        first.update(cx, |field, cx| field.select_all(window, cx));
        window.focus(&first.read(cx).focus_handle(cx), cx);
        self.schedule_rename_preview(Duration::ZERO, window, cx);
        cx.notify();
    }

    fn build_open(
        &mut self,
        batch: gather::Batch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Open {
        let form = Form::new();
        let inputs = inputs::create(&form.rules, window, cx);
        let view = cx.new(|cx| PreviewView::new(window, cx));
        let mut subscriptions = inputs::watch(&inputs, window, cx);
        subscriptions.push(
            cx.subscribe_in(&view, window, |this, _, event, window, cx| {
                this.rename_preview_event(event.clone(), window, cx);
            }),
        );
        let last_field = inputs.name_mask.read(cx).focus_handle(cx);
        Open {
            title: folder_title(&batch.folder, batch.items.len()),
            source: compute::Source {
                items: Arc::new(batch.items),
                existing: Arc::new(batch.existing),
                fold: gather::platform_fold(),
                registry: Arc::new(crate::rename::fields::builtin()),
                fields: Arc::default(),
            },
            form,
            inputs,
            view,
            preview: None,
            errors: FieldErrors::default(),
            generation: 0,
            run_waiting_for: None,
            menu: None,
            names: None,
            sorted: None,
            root: cx.focus_handle(),
            preview_focus: cx.focus_handle(),
            last_field,
            compute: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn close_multi_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.multi_rename.open.take().is_some() {
            self.focus_core_pane(window, cx);
            cx.notify();
        }
    }

    pub(in crate::window_view) fn is_multi_rename_open(&self) -> bool {
        self.multi_rename.open.is_some()
    }

    fn rules_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        let is_counter_ok = inputs::read(&open.inputs, &mut open.form.rules, cx);
        open.errors = if is_counter_ok {
            open.form
                .compile()
                .err()
                .map(|error| crate::multi_rename::field_errors(&open.form.rules, &error))
                .unwrap_or_default()
        } else {
            crate::multi_rename::counter_error()
        };
        if open.errors == FieldErrors::default() {
            self.schedule_rename_preview(compute::DEBOUNCE, window, cx);
        } else {
            open.generation += 1;
            open.compute = None;
            open.run_waiting_for = None;
            open.push_preview(cx);
        }
        cx.notify();
    }

    fn schedule_rename_preview(
        &mut self,
        delay: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        open.generation += 1;
        if open.form.names_error(open.source.items.len()).is_some() {
            open.compute = None;
            open.run_waiting_for = None;
            open.push_preview(cx);
            cx.notify();
            return;
        }
        let job = open.source.job(&open.form, open.generation);
        open.compute = Some(cx.spawn_in(window, async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let preview = cx.background_spawn(async move { job.run() }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.rename_preview_ready(preview, window, cx);
            });
        }));
    }

    fn rename_preview_ready(
        &mut self,
        preview: Option<Preview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        let Some(preview) = preview.filter(|preview| preview.generation == open.generation) else {
            return;
        };
        open.preview = Some(preview);
        open.compute = None;
        open.push_preview(cx);
        if open
            .run_waiting_for
            .take()
            .is_some_and(|form| form == open.form)
        {
            self.run_multi_rename(window, cx);
        }
        cx.notify();
    }

    fn toggle_rename_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        open.form.toggle_excluded(row);
        self.schedule_rename_preview(compute::DEBOUNCE, window, cx);
    }

    fn rename_preview_event(
        &mut self,
        event: PreviewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PreviewEvent::RowToggled { parent, name } => {
                let row = self.multi_rename.open.as_ref().and_then(|open| {
                    open.source
                        .items
                        .iter()
                        .position(|item| item.parent == parent && item.name == name)
                });
                if let Some(row) = row {
                    self.toggle_rename_row(row, window, cx);
                }
            }
            PreviewEvent::Run => self.run_multi_rename(window, cx),
            PreviewEvent::Cancel => self.close_multi_rename(window, cx),
            PreviewEvent::Sort(column) => self.sort_rename_rows(column, window, cx),
        }
    }

    fn rename_fields_event(
        &mut self,
        event: FieldsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        match event {
            FieldsEvent::Toggle(toggle) => {
                inputs::flip(&mut open.form.rules.find, toggle);
                self.rules_changed(window, cx);
            }
            FieldsEvent::Tokens(mask) => {
                self.open_rename_menu(menu::Kind::Tokens(mask), window, cx)
            }
            FieldsEvent::NameCase => self.open_rename_menu(menu::Kind::NameCase, window, cx),
            FieldsEvent::ExtensionCase => self.open_rename_menu(menu::Kind::ExtCase, window, cx),
        }
    }

    fn toggle_problems_only(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        open.form.problems_only = !open.form.problems_only;
        open.push_preview(cx);
        cx.notify();
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(super) fn multi_rename_rows(&self) -> Option<Vec<(String, String)>> {
        let open = self.multi_rename.open.as_ref()?;
        let preview = open.preview.as_ref()?;
        Some(
            open.source
                .items
                .iter()
                .zip(&preview.rows)
                .map(|(item, row)| (item.name.clone(), row.new.clone()))
                .collect(),
        )
    }

    pub(super) fn multi_rename_cursor(&self, cx: &App) -> Option<usize> {
        let open = self.multi_rename.open.as_ref()?;
        open.view.read(cx).cursor()
    }

    pub(super) fn multi_rename_inputs(&self) -> Option<FieldInputs> {
        Some(self.multi_rename.open.as_ref()?.inputs.clone())
    }

    pub(super) fn multi_rename_errors(&self) -> Option<FieldErrors> {
        Some(self.multi_rename.open.as_ref()?.errors.clone())
    }
}
