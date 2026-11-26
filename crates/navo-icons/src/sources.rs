//! In-memory readers for upstream npm icon tarballs and the set builders on top of them.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::pack::SetDraft;

mod nodes;
mod simple_icons;
mod svg_dir;
mod tarball;

#[cfg(test)]
mod tests;

/// How a node-based source draws its icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Outline icons drawn with a black stroke.
    Stroke,
    /// Solid icons drawn with a black fill.
    Fill,
}

/// Where a source's icons and metadata live inside its tarball.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reader {
    /// Lucide and Tabler node JSON files.
    Nodes {
        /// The JSON file mapping icon names to node lists.
        file: String,
        /// The drawing style for every icon in the set.
        style: Style,
        /// An optional separate file with search tags.
        tags: Option<String>,
    },
    /// A directory holding one SVG file per icon.
    SvgDir {
        /// The directory prefix inside the tarball.
        dir: String,
        /// The file suffix to keep, such as `.svg`.
        suffix: String,
        /// The name transform, such as `underscore`.
        strip: String,
        /// A filename suffix to skip, such as `-fill.svg`.
        skip_suffix: Option<String>,
    },
    /// Simple Icons with its strict licence filter.
    SimpleIcons,
}

/// One upstream icon set to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpec {
    /// The set id written into the pack.
    pub id: String,
    /// The upstream version.
    pub version: String,
    /// The licence id.
    pub licence: String,
    /// The icon grid in px.
    pub grid: u16,
    /// Where the icons live in the tarball.
    pub reader: Reader,
}

/// Why a tarball or one of its icon sets could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourceError {
    /// Gzip or tar decoding failed.
    #[error("archive error: {0}")]
    Archive(String),
    /// An entry, entry count or total size passed a safety limit.
    #[error("archive exceeds safety limits")]
    TooLarge,
    /// A file the reader needs is not in the tarball.
    #[error("missing file in archive: {0}")]
    Missing(String),
    /// A metadata file is not the expected JSON.
    #[error("invalid JSON: {0}")]
    Json(String),
    /// One built icon failed SVG normalisation.
    #[error("icon `{name}` failed sanitisation: {error}")]
    Icon {
        /// The icon being built.
        name: String,
        /// The normaliser's message.
        error: String,
    },
}

/// Reads the wanted regular files from a gzipped npm tarball without touching disk.
pub fn read_tarball(
    tgz: &[u8],
    wanted: &dyn Fn(&str) -> bool,
) -> Result<BTreeMap<String, Vec<u8>>, SourceError> {
    tarball::read(tgz, wanted)
}

/// Builds one icon set draft from the files of its tarball.
pub fn build_set(
    files: &BTreeMap<String, Vec<u8>>,
    spec: &SourceSpec,
) -> Result<SetDraft, SourceError> {
    let icons = match &spec.reader {
        Reader::Nodes { file, style, tags } => {
            nodes::build(files, file, *style, tags.as_deref(), spec.grid)?
        }
        Reader::SvgDir {
            dir,
            suffix,
            strip,
            skip_suffix,
        } => svg_dir::build(files, dir, suffix, strip, skip_suffix.as_deref())?,
        Reader::SimpleIcons => simple_icons::build(files)?,
    };
    Ok(SetDraft {
        id: spec.id.clone(),
        version: spec.version.clone(),
        licence: spec.licence.clone(),
        grid: spec.grid,
        icons,
    })
}

/// Parses one metadata file, mapping serde's message into the source error.
pub(super) fn parse_json(bytes: &[u8]) -> Result<Value, SourceError> {
    serde_json::from_slice(bytes).map_err(|error| SourceError::Json(error.to_string()))
}

/// Appends the lower-cased whitespace-split words of `text`, skipping duplicates.
pub(super) fn push_words(text: &str, out: &mut Vec<String>) {
    for word in text.split_whitespace() {
        let word = word.to_lowercase();
        if !word.is_empty() && !out.contains(&word) {
            out.push(word);
        }
    }
}

/// Normalises one built SVG, naming the icon when the normaliser refuses it.
pub(super) fn clean(name: &str, bytes: &[u8]) -> Result<Vec<u8>, SourceError> {
    crate::sanitise::normalise(bytes, true)
        .map(|cleaned| cleaned.svg)
        .map_err(|error| SourceError::Icon {
            name: name.to_owned(),
            error: error.to_string(),
        })
}
