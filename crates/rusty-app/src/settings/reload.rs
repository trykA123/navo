use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{DefaultChange, SettingsStore};
use rusty_ui::settings::DefaultShift;
use rusty_ui::window_view::SettingsNews;

use super::start::Started;
use super::{paused, report, skipped, values_of};
use crate::search_hooks::Remember;

pub(super) struct Delivery {
    reloader: Reloader,
    on_news: Box<dyn FnMut(SettingsNews) + Send>,
}

impl Delivery {
    pub(super) fn new(
        reloader: Reloader,
        on_news: impl FnMut(SettingsNews) + Send + 'static,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            reloader,
            on_news: Box::new(on_news),
        }))
    }
}

pub(super) fn deliver(delivery: &Mutex<Delivery>) {
    let mut delivery = delivery.lock().unwrap_or_else(PoisonError::into_inner);
    for news in delivery.reloader.reload_all() {
        (delivery.on_news)(news);
    }
}

pub(super) fn announce(delivery: &Mutex<Delivery>, changes: &[DefaultChange]) {
    if changes.is_empty() {
        return;
    }
    let shifts = changes
        .iter()
        .map(|change| DefaultShift {
            key: change.key.clone(),
            old: change.old.clone(),
            new: change.new.clone(),
        })
        .collect();
    let mut delivery = delivery.lock().unwrap_or_else(PoisonError::into_inner);
    (delivery.on_news)(SettingsNews::Defaults(shifts));
}

pub(super) fn receive_late(receiver: Option<Receiver<Started>>, delivery: Arc<Mutex<Delivery>>) {
    let Some(receiver) = receiver else { return };
    let spawned = std::thread::Builder::new()
        .name("navo-settings-late".to_owned())
        .spawn(move || {
            if let Ok(started) = receiver.recv() {
                deliver(&delivery);
                announce(&delivery, &started.default_changes);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "late settings thread could not start");
    }
}

pub struct Reloader {
    store: Arc<Mutex<SettingsStore>>,
    problem: Option<String>,
    remember: Option<Remember>,
    guarded: Option<PathBuf>,
    skipped: Option<String>,
}

impl Reloader {
    #[must_use]
    pub fn new(store: Arc<Mutex<SettingsStore>>, problem: Option<String>) -> Self {
        Self {
            store,
            problem,
            remember: None,
            guarded: None,
            skipped: None,
        }
    }

    #[must_use]
    pub(super) fn remembering(mut self, remember: Option<Remember>) -> Self {
        self.remember = remember;
        self
    }

    #[must_use]
    pub(super) fn guarding(mut self, guarded: Option<PathBuf>) -> Self {
        self.guarded = guarded;
        self
    }

    #[must_use]
    pub(super) fn skipping(mut self, skipped: Option<String>) -> Self {
        self.skipped = skipped;
        self
    }

    #[cfg(test)]
    pub fn reload(&mut self) -> Option<SettingsNews> {
        self.reload_all().into_iter().next()
    }

    fn reload_all(&mut self) -> Vec<SettingsNews> {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let before = store.current().clone();
        let Some(loaded) = store.reload_watched() else {
            if let Some(remember) = &self.remember {
                remember.set(store.current().search.remember);
            }
            return Vec::new();
        };
        if let Some(error) = loaded.error {
            let message = paused::describe(&error, self.guarded.as_deref());
            if self.problem.as_ref() == Some(&message) {
                return Vec::new();
            }
            report(&message);
            self.problem = Some(message.clone());
            return vec![SettingsNews::Invalid(message)];
        }
        if let Some(remember) = &self.remember {
            remember.set(store.current().search.remember);
        }
        let skipped = skipped::message(&loaded.unknown_keys);
        let was_broken = self.problem.take().is_some();
        let is_cleared = skipped.is_none() && self.skipped.is_some();
        let is_same = store.current() == &before;
        let applied = (was_broken || is_cleared || !is_same).then(|| SettingsNews::Applied {
            values: values_of(&store),
            generation: store.generation(),
        });
        let notice = skipped
            .clone()
            .filter(|message| applied.is_some() || self.skipped.as_ref() != Some(message));
        if let Some(message) = &notice {
            report(message);
        }
        self.skipped = skipped;
        applied
            .into_iter()
            .chain(notice.map(SettingsNews::Invalid))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rusty_store::SettingValue;

    use super::*;

    #[test]
    fn an_own_save_does_not_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.toml");
        std::fs::write(&path, "[appearance]\nmode = \"light\"\n").unwrap();
        let store = Arc::new(Mutex::new(SettingsStore::load(&path).0));
        let mut reloader = Reloader::new(store.clone(), None);
        store
            .lock()
            .unwrap()
            .save_preserving_edits("appearance.mode", Some(SettingValue::Text("dark".into())))
            .unwrap();
        let saved = store.lock().unwrap().generation();
        assert!(reloader.reload().is_none());
        assert!(reloader.reload().is_none());
        assert_eq!(store.lock().unwrap().generation(), saved);
    }

    #[test]
    fn an_outside_edit_after_an_own_save_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.toml");
        let store = Arc::new(Mutex::new(SettingsStore::load(&path).0));
        let mut reloader = Reloader::new(store.clone(), None);
        store
            .lock()
            .unwrap()
            .save_preserving_edits("appearance.mode", Some(SettingValue::Text("dark".into())))
            .unwrap();
        let saved = store.lock().unwrap().generation();
        std::fs::write(&path, "[appearance]\nmode = \"light\"\n").unwrap();
        assert!(matches!(
            reloader.reload(),
            Some(SettingsNews::Applied { .. })
        ));
        assert!(store.lock().unwrap().generation() > saved);
    }
}
