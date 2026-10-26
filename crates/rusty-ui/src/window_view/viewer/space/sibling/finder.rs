use rusty_core::{Location, ProviderPath};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{FileHooks, Partner, SiblingProblem, Stem};
use crate::window_view::viewer::Head;

pub(super) struct Found {
    pub name: String,
    pub bytes: Vec<u8>,
    pub only: bool,
}

pub(super) struct Finder<'a> {
    hooks: &'a FileHooks,
    stem: &'a Stem,
    cancel: &'a AtomicBool,
    names: Option<Vec<ProviderPath>>,
}

impl<'a> Finder<'a> {
    pub fn new(hooks: &'a FileHooks, stem: &'a Stem, cancel: &'a AtomicBool) -> Self {
        Self {
            hooks,
            stem,
            cancel,
            names: None,
        }
    }

    pub fn find(&mut self, partner: Partner, problems: &mut Vec<SiblingProblem>) -> Option<Found> {
        if self.cancelled()
            || partner
                .extensions()
                .iter()
                .any(|extension| extension.eq_ignore_ascii_case(&self.stem.extension))
        {
            return None;
        }
        let (name, location, head, only) = self.candidate(partner)?;
        if self.cancelled() {
            return None;
        }
        let limit = partner.limit();
        if head.len > limit {
            problems.push(SiblingProblem::TooLarge { name, limit });
            return None;
        }
        if !partner.sniff(&head.bytes) {
            problems.push(SiblingProblem::NotRecognised { name });
            return None;
        }
        match (self.hooks.bytes)(&location, limit) {
            Ok(bytes) if !self.cancelled() => Some(Found { name, bytes, only }),
            Ok(_) => None,
            Err(_) => {
                problems.push(SiblingProblem::Unreadable { name });
                None
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn candidate(&mut self, partner: Partner) -> Option<(String, Location, Head, bool)> {
        for extension in partner.extensions() {
            if self.cancelled() {
                return None;
            }
            let name = ProviderPath::from_text(&format!("{}.{extension}", self.stem.stem));
            if let Some((location, head)) = self.probe(&name) {
                return Some((name.to_text(), location, head, false));
            }
        }
        if partner == Partner::Elf || self.cancelled() {
            return None;
        }
        if self.names.is_none() {
            self.names =
                Some((self.hooks.companions)(&self.stem.folder, self.cancel).unwrap_or_default());
        }
        if self.cancelled() {
            return None;
        }
        let extension = partner.extensions()[0];
        let mut matches = self.names.as_ref()?.iter().filter(|name| {
            name.to_text()
                .rsplit_once('.')
                .is_some_and(|(_, found)| extension.eq_ignore_ascii_case(found))
        });
        let name = matches.next()?.clone();
        if matches.next().is_some() {
            return None;
        }
        let (location, head) = self.probe(&name)?;
        Some((name.to_text(), location, head, true))
    }

    fn probe(&self, name: &ProviderPath) -> Option<(Location, Head)> {
        let location = self.stem.folder.join(name).ok()?;
        if location.parent().as_ref() != Some(&self.stem.folder) {
            return None;
        }
        let head = (self.hooks.head)(&location).ok()?;
        Some((location, head))
    }
}
