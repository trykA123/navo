use std::io;
use std::path::Path;

use gix_config::File;

use crate::GitError;

use super::{ObjectFormat, RefStorage, config_error, invalid_config};

const V0_TOLERATED: &[&str] = &["noop", "preciousobjects", "partialclone", "worktreeconfig"];
const V1_ONLY: &[&str] = &[
    "noop-v1",
    "objectformat",
    "compatobjectformat",
    "refstorage",
    "relativeworktrees",
    "submodulepathconfig",
];

pub(super) struct Extensions {
    pub object_format: ObjectFormat,
    pub ref_storage: RefStorage,
    pub worktree_config: bool,
}

pub(super) fn read(config: &File, version: u32, path: &Path) -> Result<Extensions, GitError> {
    validate(config, version)?;
    let object_format = if version == 1 {
        read_object_format(config, path)?
    } else {
        ObjectFormat::Sha1
    };
    let ref_storage = if version == 1 {
        read_ref_storage(config, path)?
    } else {
        RefStorage::Files
    };
    let worktree_config = read_worktree_config(config, path)?.unwrap_or(false);
    Ok(Extensions {
        object_format,
        ref_storage,
        worktree_config,
    })
}

fn validate(config: &File, version: u32) -> Result<(), GitError> {
    for (subsection, name) in entries(config) {
        let normalized = name.to_ascii_lowercase();
        if subsection.is_none() && known(&normalized) {
            if version == 0 && V1_ONLY.contains(&normalized.as_str()) {
                return Err(GitError::Unsupported {
                    operation: format!("repository extension {normalized} at version 0"),
                });
            }
            continue;
        }
        if version == 1 {
            let name = match subsection {
                Some(subsection) => format!("{subsection}.{name}"),
                None => name,
            };
            return Err(GitError::Unsupported {
                operation: format!("repository extension {name}"),
            });
        }
    }
    Ok(())
}

fn known(name: &str) -> bool {
    V0_TOLERATED.contains(&name) || V1_ONLY.contains(&name)
}

fn entries(config: &File) -> Vec<(Option<String>, String)> {
    let Some(sections) = config.sections_by_name("extensions") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for section in sections {
        let subsection = section
            .header()
            .subsection_name()
            .map(|name| name.to_string());
        for name in section.value_names() {
            entries.push((subsection.clone(), name));
        }
    }
    entries
}

fn read_object_format(config: &File, path: &Path) -> Result<ObjectFormat, GitError> {
    match extension_value(config, "objectformat") {
        Some(None) => Err(invalid_config(path, "extensions.objectFormat has no value")),
        Some(Some(value)) => match value.as_str() {
            "sha1" => Ok(ObjectFormat::Sha1),
            "sha256" => Ok(ObjectFormat::Sha256),
            _ => Err(GitError::UnsupportedObjectFormat { value }),
        },
        None => Ok(ObjectFormat::Sha1),
    }
}

fn read_ref_storage(config: &File, path: &Path) -> Result<RefStorage, GitError> {
    match extension_value(config, "refstorage") {
        Some(None) => Err(invalid_config(path, "extensions.refStorage has no value")),
        Some(Some(value)) => match value.as_str() {
            "files" => Ok(RefStorage::Files),
            "reftable" => Ok(RefStorage::Reftable),
            _ => Err(GitError::UnsupportedRefStorage { value }),
        },
        None => Ok(RefStorage::Files),
    }
}

fn read_worktree_config(config: &File, path: &Path) -> Result<Option<bool>, GitError> {
    config
        .boolean_by("extensions", None, "worktreeconfig")
        .map_err(|error| {
            config_error(
                path,
                error.downcast_any_ref::<io::Error>().map(io::Error::kind),
                error.to_string(),
            )
        })
}

fn extension_value(config: &File, name: &str) -> Option<Option<String>> {
    let sections = config.sections_by_name("extensions")?;
    let mut value = None;
    for section in sections {
        if section.header().subsection_name().is_some() {
            continue;
        }
        if let Some(found) = section.value_implicit(name) {
            value = Some(found.map(|value| value.to_string()));
        }
    }
    value
}
