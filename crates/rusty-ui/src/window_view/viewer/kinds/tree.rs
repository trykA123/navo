use rusty_core::Location;
use rusty_structured::{SMALL_LIMIT, TREE_LIMIT, json, xml, yaml};

use super::{Loaded, Slot, Sniff, ViewerKind, extension, first_byte};
use crate::prelude::*;
use crate::window_view::viewer::FileHooks;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::tree::{Format, TreeBody, TreeFile, TreeLoad};

const STRUCTURED: u8 = 70;
const TOO_LARGE: &str = "Over 64 MB: shown as text, not as a tree.";

fn format_of(name: &str) -> Option<Format> {
    Some(match extension(name)?.as_str() {
        "json" => Format::Json,
        "jsonc" | "json5" => Format::Jsonc,
        "yaml" | "yml" => Format::Yaml,
        "xml" | "arxml" | "xsd" | "xaml" | "csproj" | "plist" | "svg" => Format::Xml,
        _ => return None,
    })
}

fn looks_right(format: Format, head: &[u8]) -> bool {
    let first = first_byte(head);
    match format {
        Format::Json => matches!(first, Some(b'{' | b'[')),
        Format::Jsonc => matches!(first, Some(b'{' | b'[' | b'/')),
        Format::Yaml => true,
        Format::Xml => first == Some(b'<'),
    }
}

fn is_too_large(format: Format, len: u64) -> bool {
    format != Format::Xml && len > TREE_LIMIT
}

fn parse(format: Format, bytes: &[u8]) -> TreeLoad {
    let parsed = match format {
        Format::Json => json::parse_json(bytes),
        Format::Jsonc => json::parse_jsonc(bytes),
        Format::Yaml => yaml::parse_yaml(bytes),
        Format::Xml => xml::parse_xml(bytes),
    };
    match parsed {
        Ok(tree) => TreeLoad::Parsed(tree),
        Err(error) => TreeLoad::Failed(error),
    }
}

pub(super) struct TreeKind;

impl ViewerKind for TreeKind {
    fn id(&self) -> &'static str {
        "tree"
    }

    fn name(&self) -> &'static str {
        "Tree"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        match format_of(sniff.name) {
            Some(format)
                if !sniff.looks_binary()
                    && looks_right(format, sniff.head)
                    && !is_too_large(format, sniff.len) =>
            {
                STRUCTURED
            }
            _ => 0,
        }
    }

    fn declined(&self, sniff: &Sniff<'_>) -> Option<&'static str> {
        let format = format_of(sniff.name)?;
        let fits = !sniff.looks_binary() && looks_right(format, sniff.head);
        (fits && is_too_large(format, sniff.len)).then_some(TOO_LARGE)
    }

    fn load(
        &self,
        hooks: &FileHooks,
        location: &Location,
        sniff: &Sniff<'_>,
    ) -> Result<Loaded, String> {
        let format = format_of(sniff.name).ok_or_else(|| "Not a tree file.".to_owned())?;
        let load = if format == Format::Xml && sniff.len > SMALL_LIMIT {
            TreeLoad::Index
        } else {
            let limit = if format == Format::Xml {
                SMALL_LIMIT
            } else {
                TREE_LIMIT
            };
            parse(format, &(hooks.bytes)(location, limit)?)
        };
        Ok(Box::new(TreeFile {
            format,
            len: sniff.len,
            load,
        }))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let file = attach
            .loaded
            .downcast::<TreeFile>()
            .map_err(|_| "The tree view got data it cannot show.".to_owned())?;
        Ok(Box::new(TreeBody::new(*file, attach.host, cx)))
    }
}
