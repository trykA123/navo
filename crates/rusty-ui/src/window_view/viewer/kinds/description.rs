use std::io::BufReader;

use rusty_automotive::{Watch, a2l, index_a2l, index_dbc, sniff_a2l, sniff_dbc};
use rusty_l10n::t;

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::background::{Ranged, ranged};
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::description::{DescModel, DescriptionBody, DescriptionFile};

pub(super) struct DbcKind;
pub(super) struct A2lKind;

impl ViewerKind for DbcKind {
    fn id(&self) -> &'static str {
        "dbc"
    }

    fn name(&self) -> &'static str {
        "DBC"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &["dbc"]) && !sniff.looks_binary() && sniff_dbc(sniff.head) {
            70
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        load.ensure_open()?;
        let model = index_dbc(
            reader(&load),
            Watch {
                cancel: load.cancel,
                progress: &mut |_| {},
            },
        )
        .map_err(|error| error.to_string())?;
        let file = DescriptionFile::new(load.sniff.len, DescModel::Dbc(model));
        load.ensure_open()?;
        Ok(Box::new(file))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        attach_description(attach, cx)
    }
}

impl ViewerKind for A2lKind {
    fn id(&self) -> &'static str {
        "a2l"
    }

    fn name(&self) -> &'static str {
        "A2L"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &["a2l"]) && !sniff.looks_binary() && sniff_a2l(sniff.head) {
            70
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        load.ensure_open()?;
        let model = index_a2l(
            reader(&load),
            Watch {
                cancel: load.cancel,
                progress: &mut |_| {},
            },
        )
        .map_err(|error| error.to_string())?;
        let links = a2l::links(&model);
        let file = DescriptionFile::new(load.sniff.len, DescModel::A2l(model, links));
        load.ensure_open()?;
        Ok(Box::new(file))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        attach_description(attach, cx)
    }
}

fn reader(load: &Load<'_>) -> impl std::io::BufRead + Send + 'static {
    BufReader::new(Ranged {
        read: ranged(load.hooks, load.location),
        at: 0,
        len: load.sniff.len,
    })
}

fn attach_description(attach: Attach, cx: &mut App) -> Result<Box<dyn ViewerBody>, String> {
    let file = attach
        .loaded
        .downcast::<DescriptionFile>()
        .map_err(|_| t!("viewer-description-bad-data"))?;
    Ok(Box::new(DescriptionBody::new(*file, attach.host, cx)))
}

#[cfg(test)]
mod tests;
