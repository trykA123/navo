use rusty_automotive::space::Kind;
use rusty_l10n::{t, tr};

use super::StripData;
use super::cache::{Item, Over};
use crate::format;
use crate::prelude::SharedString;

pub(in crate::window_view::viewer) fn kind_name(kind: Kind) -> String {
    tr(match kind {
        Kind::Code => "space-kind-code",
        Kind::Rodata => "space-kind-rodata",
        Kind::Data => "space-kind-data",
        Kind::Bss => "space-kind-bss",
        Kind::Other => "space-kind-other",
        Kind::Fill => "space-kind-fill",
    })
}

fn facts(data: &StripData, item: Item) -> Option<(Kind, u64, u64)> {
    match item {
        Item::Block(at) => {
            let block = data.region.blocks.get(at)?;
            Some((block.kind, block.size, block.start))
        }
        Item::Owner(at) => {
            let owner = data.owners.as_ref()?.items.get(at)?;
            Some((owner.kind, owner.size, owner.start))
        }
    }
}

pub(super) fn text(data: &StripData, over: &Over) -> SharedString {
    let text = match over {
        Over::Sections { count, bytes } => {
            t!(
                "space-strip-sections",
                count = *count,
                size = format::size(*bytes)
            )
        }
        Over::Symbols { count, bytes } => {
            t!(
                "space-strip-symbols",
                count = *count,
                size = format::size(*bytes)
            )
        }
        Over::One(item) => one(data, *item),
    };
    text.into()
}

fn one(data: &StripData, item: Item) -> String {
    let label = (data.describe)(item);
    let Some((kind, bytes, start)) = facts(data, item) else {
        return label.name.to_string();
    };
    let name = label.name.to_string();
    let kind = kind_name(kind);
    let size = format::size(bytes);
    let address = format!("0x{start:08X}");
    match label.object {
        Some(object) => t!(
            "space-strip-tip-object",
            name = name,
            kind = kind,
            size = size,
            address = address,
            object = object.to_string()
        ),
        None => t!(
            "space-strip-tip",
            name = name,
            kind = kind,
            size = size,
            address = address
        ),
    }
}
