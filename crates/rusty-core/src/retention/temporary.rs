use serde::{Deserialize, Serialize};

use super::{RetainedWritePlan, RetentionError, validation};
use crate::{FileIdentity, Location, ProviderPath, Step, TEMP_SUFFIX};

pub(super) fn validate_namespace(namespace: &str) -> Result<(), RetentionError> {
    if namespace.is_empty()
        || namespace.len() > 80
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(RetentionError::Invalid("invalid copy temporary namespace"));
    }
    Ok(())
}

pub fn copy_temporary(
    target: &Location,
    seq: u32,
    namespace: Option<&str>,
) -> Result<Location, RetentionError> {
    let parent = target
        .parent()
        .ok_or(RetentionError::Invalid("copy target has no parent"))?;
    let name = match namespace {
        Some(namespace) => {
            validate_namespace(namespace)?;
            format!("{TEMP_SUFFIX}-{namespace}-{seq:x}")
        }
        None => format!(
            "{}{TEMP_SUFFIX}",
            target
                .name()
                .ok_or(RetentionError::Invalid("copy target has no name"))?
                .to_text()
        ),
    };
    parent
        .join(&ProviderPath::from_text(&name))
        .map_err(|_| RetentionError::Invalid("invalid copy temporary path"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyTemporary {
    pub temporary_version: u32,
    pub seq: u32,
    pub target: Location,
    pub temporary: Location,
    pub parent_identity: FileIdentity,
    pub mount_identity: u64,
    pub identity: Option<FileIdentity>,
}

impl CopyTemporary {
    pub fn validate(&self, owner: &RetainedWritePlan) -> Result<(), RetentionError> {
        let namespace = owner.temporary_namespace().ok_or(RetentionError::Invalid(
            "copy temporary has no durable namespace",
        ))?;
        let step = usize::try_from(self.seq)
            .ok()
            .and_then(|seq| owner.plan().steps.get(seq));
        let matches = matches!(step, Some(Step::CopyFile {target, ..}) if target == &self.target);
        if self.temporary.same_place(&self.target)
            || self.temporary_version != 1
            || !matches
            || self.temporary != copy_temporary(&self.target, self.seq, Some(namespace))?
            || !validation::is_absolute(&self.target)
            || !validation::clean_path(&self.target)
            || self
                .identity
                .is_some_and(|identity| identity.volume != self.parent_identity.volume)
        {
            return Err(RetentionError::Invalid(
                "copy temporary does not match its owner or scope",
            ));
        }
        Ok(())
    }
}
