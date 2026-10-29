use std::collections::HashMap;

use super::{
    ADOPTED_CONTAINER_PREFIX, CONTAINER_PREFIX, DAY_MS, MAX_NODES, RetainedKind, RetainedState,
    RetentionEntry, RetentionError, RetentionRecord, VAULT_RECORD_VERSION,
};
use crate::{Location, brand};

pub fn is_absolute(location: &Location) -> bool {
    !location.is_local() || location.as_str().starts_with(location.root().as_str())
}

pub(super) fn clean_path(location: &Location) -> bool {
    std::iter::successors(Some(location.clone()), Location::parent)
        .filter_map(|part| part.name())
        .all(|part| !matches!(part.to_text().as_str(), "." | ".."))
}

impl RetentionRecord {
    pub fn validate(&self) -> Result<(), RetentionError> {
        if !matches!(self.version, 1..=VAULT_RECORD_VERSION)
            || !is_absolute(&self.source)
            || !clean_path(&self.source)
            || self.source.parent().is_none()
        {
            return Err(RetentionError::Invalid("unsupported record or source path"));
        }
        self.validate_payload()?;
        self.validate_container()?;
        let age = self.expires_at.checked_sub(self.created_at);
        if self.created_at < 0 || !age.is_some_and(|age| (DAY_MS..=7 * DAY_MS).contains(&age)) {
            return Err(RetentionError::Invalid("invalid retention interval"));
        }
        if self
            .container_identity
            .is_some_and(|id| id.volume != self.parent_identity.volume)
        {
            return Err(RetentionError::Invalid(
                "container is on a different volume",
            ));
        }
        self.validate_nodes()?;
        self.bytes()?;
        Ok(())
    }

    fn validate_container(&self) -> Result<(), RetentionError> {
        let prefixes = if self.version != 2 {
            [CONTAINER_PREFIX, brand::LEGACY_RETAINED_PREFIX]
        } else {
            [ADOPTED_CONTAINER_PREFIX, brand::LEGACY_HELD_PREFIX]
        };
        let name = self.container.name().map(|name| name.to_text());
        let valid_name = name.as_ref().is_some_and(|name| {
            prefixes
                .iter()
                .find_map(|prefix| name.strip_prefix(prefix))
                .is_some_and(|tail| {
                    !tail.is_empty()
                        && tail.len() <= 100
                        && tail
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
                })
        });
        let placed = if self.version == VAULT_RECORD_VERSION {
            self.in_vault()
        } else {
            self.container.parent() == self.source.parent()
        };
        if !valid_name || !placed {
            return Err(RetentionError::Invalid(
                "container is not in a vault or a reserved sibling",
            ));
        }
        Ok(())
    }

    pub fn validate_in(&self, data: Option<&Location>) -> Result<(), RetentionError> {
        self.validate()?;
        let vault = self.container.parent();
        let in_profile_vault = self.version == VAULT_RECORD_VERSION
            && vault.as_ref().and_then(name_of).as_deref() == Some(brand::VAULT_FALLBACK_NAMES[0]);
        if in_profile_vault
            && !data.is_some_and(|data| {
                vault
                    .and_then(|vault| vault.parent())
                    .is_some_and(|home| home.same_place(data))
            })
        {
            return Err(RetentionError::Invalid(
                "profile vault is outside this profile",
            ));
        }
        Ok(())
    }

    fn in_vault(&self) -> bool {
        let named = self
            .container
            .parent()
            .is_some_and(|vault| match name_of(&vault).as_deref() {
                Some(name) if name.eq_ignore_ascii_case(brand::VAULT_NAME) => true,
                Some(name) if name == brand::VAULT_FALLBACK_NAMES[1] => vault
                    .parent()
                    .and_then(|trash| name_of(&trash))
                    .is_some_and(|trash| is_user_trash(&trash)),
                Some(name) => name == brand::VAULT_FALLBACK_NAMES[0],
                None => false,
            });
        named
            && self.container.provider() == self.source.provider()
            && is_absolute(&self.container)
            && clean_path(&self.container)
            && !std::iter::successors(Some(self.container.clone()), Location::parent)
                .any(|level| level.same_place(&self.source))
    }

    fn validate_payload(&self) -> Result<(), RetentionError> {
        let valid = match self.version {
            1 | VAULT_RECORD_VERSION => self.payload_name.is_none(),
            2 => {
                self.container_identity.is_some()
                    && self.nodes.len() == 1
                    && self.nodes[0].kind != RetainedKind::Folder
                    && self.source.name().is_some_and(|name| {
                        self.payload_name.as_deref() == Some(name.to_text().as_str())
                    })
            }
            _ => false,
        };
        if !valid {
            return Err(RetentionError::Invalid(
                "invalid retained payload name or kind",
            ));
        }
        self.held()?;
        Ok(())
    }

    fn validate_nodes(&self) -> Result<(), RetentionError> {
        if self.nodes.is_empty() || self.nodes.len() > MAX_NODES || !self.nodes[0].path.is_empty() {
            return Err(RetentionError::Invalid(
                "invalid retention node count or root",
            ));
        }
        let mut known = HashMap::new();
        for node in &self.nodes {
            node.location(&self.source)?;
            if node.identity.volume != self.parent_identity.volume
                || (node.kind == RetainedKind::Folder && node.size != 0)
                || known.insert(node.path.clone(), node.kind).is_some()
            {
                return Err(RetentionError::Invalid(
                    "invalid or duplicate retained node",
                ));
            }
            if !node.path.is_empty()
                && known.get(&node.path[..node.path.len() - 1]) != Some(&RetainedKind::Folder)
            {
                return Err(RetentionError::Invalid(
                    "missing parent folder in retained tree",
                ));
            }
        }
        Ok(())
    }
}

impl RetentionEntry {
    pub fn validate(&self) -> Result<(), RetentionError> {
        self.record.validate()?;
        if self.record.version == 2 && self.job_id.is_none() && !self.state.is_terminal() {
            return Err(RetentionError::Invalid(
                "adopted retained data needs its owner",
            ));
        }
        if self.id.0 <= 0 || self.job_id.is_some_and(|id| id <= 0) {
            return Err(RetentionError::Invalid("invalid entry or job identifier"));
        }
        if !matches!(
            self.state,
            RetainedState::Preparing | RetainedState::Aborted | RetainedState::Conflict
        ) && self.record.container_identity.is_none()
        {
            return Err(RetentionError::Invalid("missing container identity"));
        }
        let needs_target = matches!(
            self.state,
            RetainedState::Restoring | RetainedState::Restored
        );
        let allows_target = needs_target || self.state == RetainedState::Conflict;
        if (needs_target && self.target.is_none()) || (!allows_target && self.target.is_some()) {
            return Err(RetentionError::Invalid("restore state and target disagree"));
        }
        if let Some(target) = &self.target {
            let volume = self.record.parent_identity.volume;
            if !is_absolute(&target.location)
                || !clean_path(&target.location)
                || target.location.provider() != self.record.source.provider()
                || target.parent_identity.volume != volume
                || std::iter::successors(Some(target.location.clone()), Location::parent)
                    .any(|parent| parent.same_place(&self.record.container))
            {
                return Err(RetentionError::Invalid("invalid restore target"));
            }
        }
        Ok(())
    }
}

fn name_of(location: &Location) -> Option<String> {
    location.name().map(|name| name.to_text())
}

fn is_user_trash(name: &str) -> bool {
    name.strip_prefix(".Trash-")
        .is_some_and(|user| !user.is_empty() && user.bytes().all(|byte| byte.is_ascii_digit()))
}
