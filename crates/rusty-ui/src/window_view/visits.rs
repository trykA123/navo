use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::FerriteWindow;
use crate::browser::Seen;
use crate::prelude::*;

const REPEAT: Duration = Duration::from_secs(2);
const PRUNE_AT: usize = 64;

#[derive(Default)]
pub(super) struct Visits {
    folders: HashMap<String, Instant>,
}

impl Visits {
    pub(super) fn is_due(&mut self, folder: &str, now: Instant) -> bool {
        if let Some(last) = self.folders.get(folder)
            && now.saturating_duration_since(*last) < REPEAT
        {
            return false;
        }
        if self.folders.len() >= PRUNE_AT {
            self.folders
                .retain(|_, last| now.saturating_duration_since(*last) < REPEAT);
        }
        self.folders.insert(folder.to_owned(), now);
        true
    }
}

impl FerriteWindow {
    pub(super) fn note_seen(&mut self, cx: &mut Context<Self>) {
        for seen in self.browser.take_seen() {
            match seen {
                Seen::Folder(location) => self.note_folder(location.as_str(), Instant::now(), cx),
                Seen::File(location) => self.record_visit(location.as_str(), false, cx),
            }
        }
    }

    pub(super) fn note_file(&self, location: &rusty_core::Location, cx: &mut Context<Self>) {
        self.record_visit(location.as_str(), false, cx);
    }

    pub(super) fn note_folder(&mut self, folder: &str, now: Instant, cx: &mut Context<Self>) {
        if self.command_center.visits.is_due(folder, now) {
            self.record_visit(folder, true, cx);
        }
    }

    fn record_visit(&self, location: &str, is_folder: bool, cx: &mut Context<Self>) {
        let record = self.command_center.hooks.record_visit.clone();
        let location = location.to_owned();
        cx.background_executor()
            .spawn(async move { record(&location, is_folder) })
            .detach();
    }
}
