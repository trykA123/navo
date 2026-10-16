mod archive;
mod font;
mod hex;
mod image;
mod text;

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;

use super::FileHooks;
use super::body::{Attach, ViewerBody};
use crate::prelude::*;

pub const EDIT_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Text = 1,
    Hex = 3,
    Rich = 5,
}

impl Slot {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Text => "1",
            Self::Hex => "3",
            Self::Rich => "5",
        }
    }

    pub(super) fn alt_key(self) -> &'static str {
        match self {
            Self::Text => "Alt+1",
            Self::Hex => "Alt+3",
            Self::Rich => "Alt+5",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Sniff<'a> {
    pub head: &'a [u8],
    pub name: &'a str,
    pub len: u64,
}

impl Sniff<'_> {
    #[must_use]
    pub fn looks_binary(&self) -> bool {
        rusty_core::encoding::looks_binary(self.head)
    }
}

pub(super) use image::{IMAGE_ID, is_image_name};
pub(super) use text::paged_text;

pub type Loaded = Box<dyn Any + Send>;

#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Load<'a> {
    pub hooks: &'a FileHooks,
    pub location: &'a Location,
    pub sniff: &'a Sniff<'a>,
    pub cancel: &'a AtomicBool,
}

pub trait ViewerKind: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn slot(&self) -> Slot;
    fn score(&self, sniff: &Sniff<'_>) -> u8;
    fn load(&self, load: Load<'_>) -> Result<Loaded, String>;
    fn attach(
        &self,
        attach: Attach,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String>;

    fn editable(&self, _: &Sniff<'_>) -> bool {
        false
    }

    fn reuse(&self, _: &dyn ViewerBody, _: &Sniff<'_>, _: &mut App) -> bool {
        false
    }
}

pub type Kind = Arc<dyn ViewerKind>;

#[derive(Clone)]
pub struct Kinds(Vec<Kind>);

impl Kinds {
    #[must_use]
    pub fn builtin() -> Self {
        Self(vec![
            Arc::new(text::Text),
            Arc::new(hex::Hex),
            Arc::new(image::Image),
            Arc::new(archive::Archive),
            Arc::new(font::Font),
        ])
    }

    #[must_use]
    pub fn with(mut self, kind: impl ViewerKind + 'static) -> Self {
        self.0.push(Arc::new(kind));
        self
    }

    #[must_use]
    pub fn choose(&self, sniff: &Sniff<'_>) -> Vec<Kind> {
        let mut scored: Vec<(u8, Kind)> = self
            .0
            .iter()
            .map(|kind| (kind.score(sniff), kind.clone()))
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.into_iter().map(|(_, kind)| kind).collect()
    }
}

#[cfg(test)]
mod tests;
