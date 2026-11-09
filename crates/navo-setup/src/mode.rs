//! Pure installer discovery decisions and bounded sq.version parsing for Navo Setup.

use std::{cmp::Ordering, fmt, path::PathBuf};

use navo_core::FileIdentity;
use navo_update::{Version, parse_version};
use roxmltree::Node;

use crate::marker::Marker;

/// Parsed package identity, also used to verify an embedded payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqVersion {
    pub pack_id: String,
    pub version: Version,
}

/// A bounded XML or JSON document could not be accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeError {
    TooLarge,
    Xml,
    Metadata,
    Version,
    Marker,
}

impl fmt::Display for ModeError {
    /// Describes the failed document check without exposing untrusted contents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge => "installer metadata exceeds its byte limit",
            Self::Xml => "sq.version is not valid XML",
            Self::Metadata => "sq.version has missing or invalid package metadata",
            Self::Version => "sq.version has an invalid semantic version",
            Self::Marker => "navo-install.json has invalid fields or identity",
        })
    }
}

impl std::error::Error for ModeError {}

/// Checked filesystem facts supplied by discovery; no filesystem access happens here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub root: PathBuf,
    pub identity: FileIdentity,
    /// All IW6 syntax and IW7/IW8a chain checks passed in the platform probe.
    pub safe_chain: bool,
    /// The probe found a regular Update.exe, rather than a directory or reparse point.
    pub update_exe: bool,
    pub sq_version: Result<SqVersion, ModeError>,
    pub marker: Option<Result<Marker, ModeError>>,
}

/// Registry absence differs from an unreadable key, missing location or invalid target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    pub registry: Option<Result<Install, Why>>,
    pub legacy: Option<Install>,
}

/// Fail-closed reasons retained for the wizard's Ambiguous page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    InvalidFolder,
    MultipleInstalls,
    DamagedVersion,
    ForeignVersion,
    MissingMarker,
    InvalidMarker,
    UnsafeName { root: PathBuf, character: char },
}

/// A single installation is carried forward so its root and marker identity stay locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Fresh,
    Upgrade(Install),
    Same(Install),
    Newer(Install),
    Ambiguous(Why),
}

/// Parses at most 64 KiB of sq.version XML, refusing damaged metadata or semver.
pub fn parse_sq_version(text: &str) -> Result<SqVersion, ModeError> {
    if text.len() > 64 * 1024 {
        return Err(ModeError::TooLarge);
    }
    let document = roxmltree::Document::parse(text).map_err(|_| ModeError::Xml)?;
    let package = document.root_element();
    if package.tag_name().name() != "package" {
        return Err(ModeError::Metadata);
    }
    let metadata = unique_child(package, "metadata")?;
    let pack_id = field_text(unique_child(metadata, "id")?)?;
    let version = field_text(unique_child(metadata, "version")?)?;
    Ok(SqVersion {
        pack_id,
        version: parse_version(&version).map_err(|_| ModeError::Version)?,
    })
}

/// Selects a mode from discovery facts and the payload version without performing I/O.
pub fn mode(discovery: &Discovery, own: &Version) -> Mode {
    let registry = discovery
        .registry
        .as_ref()
        .and_then(|found| found.as_ref().ok());
    let legacy = discovery
        .legacy
        .as_ref()
        .filter(|found| found.update_exe && found.sq_version.is_ok());
    if let Some(why) = registry.into_iter().chain(legacy).find_map(unsafe_name) {
        return Mode::Ambiguous(why);
    }
    let legacy = legacy.filter(|found| validate_install(found, true).is_ok());
    let found = match &discovery.registry {
        Some(Err(why)) => return Mode::Ambiguous(why.clone()),
        Some(Ok(found)) => {
            let is_legacy = legacy.is_some_and(|old| old.identity == found.identity);
            if let Err(why) = validate_install(found, is_legacy) {
                return Mode::Ambiguous(why);
            }
            if legacy.is_some_and(|old| old.identity != found.identity) {
                return Mode::Ambiguous(Why::MultipleInstalls);
            }
            found
        }
        None => match legacy {
            Some(found) => found,
            None => return Mode::Fresh,
        },
    };
    match found
        .sq_version
        .as_ref()
        .map(|sq| sq.version.cmp_precedence(own))
    {
        Ok(Ordering::Less) => Mode::Upgrade(found.clone()),
        Ok(Ordering::Equal) => Mode::Same(found.clone()),
        Ok(Ordering::Greater) => Mode::Newer(found.clone()),
        Err(_) => Mode::Ambiguous(Why::DamagedVersion),
    }
}

/// Retains the root and first dangerous character for the special unsafe-name advice.
fn unsafe_name(found: &Install) -> Option<Why> {
    let character = found
        .root
        .to_string_lossy()
        .chars()
        .find(|c| "%^&!".contains(*c))?;
    Some(Why::UnsafeName {
        root: found.root.clone(),
        character,
    })
}

/// Finds exactly one direct child by local name; missing or duplicate fields fail.
fn unique_child<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Result<Node<'a, 'input>, ModeError> {
    let mut matches = node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == name);
    let child = matches.next().ok_or(ModeError::Metadata)?;
    if matches.next().is_some() {
        return Err(ModeError::Metadata);
    }
    Ok(child)
}

/// Reads trimmed field text, rejecting empty fields or nested markup.
fn field_text(node: Node<'_, '_>) -> Result<String, ModeError> {
    if node.children().any(|child| child.is_element()) {
        return Err(ModeError::Metadata);
    }
    let text: String = node
        .children()
        .filter(|child| child.is_text())
        .filter_map(|child| child.text())
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Err(ModeError::Metadata);
    }
    Ok(text.to_owned())
}

/// Validates one root's facts; only legacy identity permits absent or damaged markers.
fn validate_install(found: &Install, is_legacy: bool) -> Result<(), Why> {
    if !found.safe_chain || !found.update_exe {
        return Err(Why::InvalidFolder);
    }
    let sq = found.sq_version.as_ref().map_err(|_| Why::DamagedVersion)?;
    if sq.pack_id != "NavoApp" {
        return Err(Why::ForeignVersion);
    }
    if is_legacy {
        return Ok(());
    }
    match &found.marker {
        Some(Ok(marker)) if marker.pack_id == "NavoApp" => Ok(()),
        Some(_) => Err(Why::InvalidMarker),
        None => Err(Why::MissingMarker),
    }
}
