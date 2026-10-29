use super::FerriteWindow;
use crate::prelude::*;
use crate::prompt_dialog::{PromptDialog, PromptProps};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PromptKind {
    Rename {
        from: String,
    },
    NewFolder,
    NewFile,
    Select {
        add: bool,
    },
    ChooseEditor {
        target: rusty_core::Location,
        program: String,
    },
}

pub(super) struct Prompt {
    pub(super) id: u64,
    kind: PromptKind,
    pub(super) error: Option<SharedString>,
    pub(super) is_pending: bool,
}

const NEW_FOLDER: &str = "New folder";
const NEW_FILE: &str = "New file.txt";
const EVERY_FILE: &str = "*.*";

pub(super) fn stem_end(name: &str) -> usize {
    name.rfind('.').filter(|dot| *dot > 0).unwrap_or(name.len())
}

pub(super) fn prompt_props(kind: &PromptKind, error: Option<SharedString>) -> PromptProps {
    match kind {
        PromptKind::Rename { from } => PromptProps {
            title: format!("Rename {from}").into(),
            label: "New name".into(),
            value: from.clone().into(),
            select: 0..stem_end(from),
            confirm_label: "Rename".into(),
            error,
        },
        PromptKind::NewFolder => PromptProps {
            title: "New folder".into(),
            label: "Folder name".into(),
            value: NEW_FOLDER.into(),
            select: 0..NEW_FOLDER.len(),
            confirm_label: "Create".into(),
            error,
        },
        PromptKind::NewFile => PromptProps {
            title: "New file".into(),
            label: "File name".into(),
            value: NEW_FILE.into(),
            select: 0..stem_end(NEW_FILE),
            confirm_label: "Create".into(),
            error,
        },
        PromptKind::Select { add } => PromptProps {
            title: if *add {
                "Select files"
            } else {
                "Unselect files"
            }
            .into(),
            label: "Pattern".into(),
            value: EVERY_FILE.into(),
            select: 0..EVERY_FILE.len(),
            confirm_label: if *add { "Select" } else { "Unselect" }.into(),
            error,
        },
        PromptKind::ChooseEditor { program, .. } => PromptProps {
            title: "Choose the program F4 opens files with".into(),
            label: "Program".into(),
            value: program.clone().into(),
            select: 0..program.len(),
            confirm_label: "Open".into(),
            error,
        },
    }
}

impl FerriteWindow {
    pub(super) fn ask_rename(&mut self, row: u64) {
        let name = self
            .browser
            .state()
            .active_tab()
            .listing()
            .and_then(|listing| listing.get(row))
            .map(|entry| entry.name.clone());
        if let Some(from) = name {
            self.open_prompt(PromptKind::Rename { from });
        }
    }

    pub(super) fn ask_pattern(&mut self, add: bool) {
        self.open_prompt(PromptKind::Select { add });
    }

    pub(super) fn ask_new_folder(&mut self) {
        self.open_prompt(PromptKind::NewFolder);
    }

    pub(super) fn ask_new_file(&mut self) {
        self.open_prompt(PromptKind::NewFile);
    }

    pub(super) fn open_prompt(&mut self, kind: PromptKind) {
        self.next_prompt += 1;
        self.prompt = Some(Prompt {
            id: self.next_prompt,
            kind,
            error: None,
            is_pending: false,
        });
    }

    pub(super) fn prompt_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let prompt = self.prompt.as_ref()?;
        let props = prompt_props(&prompt.kind, prompt.error.clone());
        let (submit, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = PromptDialog::new(("prompt", prompt.id), props)
            .on_submit(move |value, window, cx| {
                let value = value.to_string();
                let _ = submit.update(cx, |view, cx| view.submit_prompt(&value, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_prompt(window, cx));
            });
        Some(
            div()
                .id("prompt-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui_kit::black().opacity(0.4))
                .occlude()
                .child(dialog)
                .into_any_element(),
        )
    }

    fn submit_prompt(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = &self.prompt else {
            return;
        };
        let result = match &prompt.kind {
            PromptKind::Rename { from } => self.browser.rename(from, value).map_err(|p| match p {
                crate::browser::NameProblem::Unchanged => None,
                other => Some(other.to_string()),
            }),
            PromptKind::NewFolder => self
                .browser
                .new_folder(value)
                .map_err(|p| Some(p.to_string())),
            PromptKind::NewFile => self
                .browser
                .new_file(value)
                .map_err(|p| Some(p.to_string())),
            PromptKind::Select { add } => self.mark_pattern(*add, value),
            PromptKind::ChooseEditor { target, .. } => {
                let target = target.clone();
                self.choose_editor((value, target), window, cx);
                return;
            }
        };
        match result {
            Ok(()) | Err(None) => self.close_prompt(window, cx),
            Err(Some(problem)) => {
                if let Some(prompt) = &mut self.prompt {
                    prompt.error = Some(problem.into());
                }
                cx.notify();
            }
        }
    }

    fn mark_pattern(&mut self, add: bool, pattern: &str) -> Result<(), Option<String>> {
        let pattern = pattern.to_owned();
        let folders = self.values.patterns_select_folders();
        let command = if add {
            rusty_core::Command::SelectionAddPattern { pattern, folders }
        } else {
            rusty_core::Command::SelectionRemovePattern { pattern, folders }
        };
        match self.browser.command(&command) {
            Err(rusty_core::CommandError::Pattern(_)) => {
                Err(Some("Type a pattern, like *.txt".to_owned()))
            }
            _ => Ok(()),
        }
    }

    pub(super) fn close_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}
