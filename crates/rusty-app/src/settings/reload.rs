use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{DefaultChange, SettingsStore};
use rusty_ui::settings::DefaultShift;
use rusty_ui::window_view::SettingsNews;

use super::start::Started;
use super::{paused, report, values_of};
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
    if let Some(news) = delivery.reloader.reload() {
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
}

impl Reloader {
    #[must_use]
    pub fn new(store: Arc<Mutex<SettingsStore>>, problem: Option<String>) -> Self {
        Self {
            store,
            problem,
            remember: None,
            guarded: None,
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

    pub fn reload(&mut self) -> Option<SettingsNews> {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let before = store.current().clone();
        if let Some(error) = store.reload().error {
            let message = paused::describe(&error, self.guarded.as_deref());
            if self.problem.as_ref() == Some(&message) {
                return None;
            }
            report(&message);
            self.problem = Some(message.clone());
            return Some(SettingsNews::Invalid(message));
        }
        if let Some(remember) = &self.remember {
            remember.set(store.current().search.remember);
        }
        let was_broken = self.problem.take().is_some();
        let is_same = store.current() == &before;
        (was_broken || !is_same).then(|| SettingsNews::Applied {
            values: values_of(&store),
            generation: store.generation(),
        })
    }
}
