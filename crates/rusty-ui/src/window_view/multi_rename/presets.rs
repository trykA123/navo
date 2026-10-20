use std::sync::Arc;

use rusty_core::rename::RenameRules;
use rusty_l10n::t;

use super::menu::Kind;
use super::{FerriteWindow, inputs};
use crate::browser::{RenameHooks, RenamePresetList};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

const LAST_USED: &str = "";

fn no_hooks() -> RenameHooks {
    RenameHooks {
        last: Arc::new(|| None),
        undo_log: Arc::new(|_| Err("no journal".to_owned())),
        presets: Arc::new(|| Ok(RenamePresetList::default())),
        save: Arc::new(|_, _| Ok(())),
        delete: Arc::new(|_| Ok(())),
    }
}

impl FerriteWindow {
    fn preset_hooks(&self) -> RenameHooks {
        self.browser
            .rename_hooks()
            .cloned()
            .unwrap_or_else(no_hooks)
    }

    pub(super) fn ask_presets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_presets(Kind::Presets, window, cx);
    }

    pub(super) fn ask_save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_presets(Kind::SavePreset, window, cx);
    }

    fn open_presets(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        if self.multi_rename.open.is_none() {
            return;
        }
        self.remember_rename_field(window, cx);
        let list = self.preset_hooks().presets;
        cx.spawn_in(window, async move |this, cx| {
            let presets = cx.background_spawn(async move { list() }).await;
            let _ = this.update_in(cx, |view, window, cx| match presets {
                Ok(presets) => view.open_rename_menu_with((kind, presets), window, cx),
                Err(reason) => view.preset_failed(&reason, cx),
            });
        })
        .detach();
    }

    pub(super) fn load_preset(&mut self, json: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        let rules = match serde_json::from_str::<RenameRules>(json) {
            Ok(rules) => rules,
            Err(error) => return self.preset_failed(&error.to_string(), cx),
        };
        open.form.load_preset(&rules);
        inputs::fill(&open.inputs, &open.form.rules, window, cx);
        self.rules_changed(window, cx);
    }

    pub(super) fn save_preset(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let rules = open.form.rules();
        self.store_preset(name, &rules, cx);
    }

    pub(super) fn save_last_used(&mut self, rules: &RenameRules, cx: &mut Context<Self>) {
        self.store_preset(LAST_USED.to_owned(), rules, cx);
    }

    fn store_preset(&mut self, name: String, rules: &RenameRules, cx: &mut Context<Self>) {
        let json = match serde_json::to_string(rules) {
            Ok(json) => json,
            Err(error) => return self.preset_failed(&error.to_string(), cx),
        };
        let save = self.preset_hooks().save;
        cx.spawn(async move |this, cx| {
            let saved = cx.background_spawn(async move { save(&name, &json) }).await;
            if let Err(reason) = saved {
                let _ = this.update(cx, |view, cx| view.preset_failed(&reason, cx));
            }
        })
        .detach();
    }

    pub(super) fn delete_preset(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delete = self.preset_hooks().delete;
        cx.spawn_in(window, async move |this, cx| {
            let deleted = cx.background_spawn(async move { delete(&name) }).await;
            let _ = this.update_in(cx, |view, window, cx| match deleted {
                Ok(()) => view.ask_presets(window, cx),
                Err(reason) => view.preset_failed(&reason, cx),
            });
        })
        .detach();
    }

    fn preset_failed(&mut self, reason: &str, cx: &mut Context<Self>) {
        self.show_notice(StatusNotice {
            text: t!("rename-presets-failed", reason = reason.to_owned()).into(),
            is_error: true,
        });
        cx.notify();
    }
}
