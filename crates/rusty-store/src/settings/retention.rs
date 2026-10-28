use rusty_core::retention::{RetentionError, RetentionPolicy};
use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RetentionData", into = "RetentionData")]
pub struct RetentionSettings {
    retain_deletes: bool,
    retain_overwrites: bool,
    policy: RetentionPolicy,
}

#[derive(Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct RetentionData {
    retain_deletes: bool,
    retain_overwrites: bool,
    days: u8,
    max_volume_mib: u64,
    min_free_mib: u64,
}

impl Default for RetentionData {
    fn default() -> Self {
        RetentionSettings::default().into()
    }
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self {
            retain_deletes: true,
            retain_overwrites: true,
            policy: RetentionPolicy::default(),
        }
    }
}

impl RetentionSettings {
    #[must_use]
    pub fn off() -> Self {
        Self {
            retain_deletes: false,
            retain_overwrites: false,
            policy: RetentionPolicy::default(),
        }
    }

    pub fn policy(&self) -> RetentionPolicy {
        self.policy
    }

    pub fn maintenance_policy(&self) -> Option<RetentionPolicy> {
        (self.retain_deletes || self.retain_overwrites).then_some(self.policy)
    }

    pub fn delete_policy(&self) -> Option<RetentionPolicy> {
        self.retain_deletes.then_some(self.policy)
    }
    pub fn overwrite_policy(&self) -> Option<RetentionPolicy> {
        self.retain_overwrites.then_some(self.policy)
    }
}

impl TryFrom<RetentionData> for RetentionSettings {
    type Error = RetentionError;

    fn try_from(data: RetentionData) -> Result<Self, Self::Error> {
        let max = data
            .max_volume_mib
            .checked_mul(MIB)
            .ok_or(RetentionError::Overflow)?;
        let min = data
            .min_free_mib
            .checked_mul(MIB)
            .ok_or(RetentionError::Overflow)?;
        Ok(Self {
            retain_deletes: data.retain_deletes,
            retain_overwrites: data.retain_overwrites,
            policy: RetentionPolicy::new(data.days, max, min)?,
        })
    }
}

impl From<RetentionSettings> for RetentionData {
    fn from(settings: RetentionSettings) -> Self {
        Self {
            retain_deletes: settings.retain_deletes,
            retain_overwrites: settings.retain_overwrites,
            days: settings.policy.days(),
            max_volume_mib: settings.policy.max_volume_bytes() / MIB,
            min_free_mib: settings.policy.min_free_bytes() / MIB,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Settings, parse, to_toml};
    use super::{MIB, RetentionSettings};

    #[test]
    fn deletes_and_overwrites_are_retained_by_default_for_three_days() {
        let defaults = Settings::default().retention;
        let policy = defaults.delete_policy().unwrap();
        assert_eq!(defaults.overwrite_policy(), Some(policy));
        assert_eq!(
            (
                policy.days(),
                policy.max_volume_bytes(),
                policy.min_free_bytes()
            ),
            (3, 1024 * MIB, 256 * MIB)
        );
        assert_eq!(parse("").unwrap().0.retention, defaults);
        assert_eq!(
            parse("[retention]\ndays = 7\n")
                .unwrap()
                .0
                .retention
                .delete_policy()
                .unwrap()
                .days(),
            7
        );
    }

    #[test]
    fn off_retains_nothing() {
        let off = RetentionSettings::off();
        assert!(off.delete_policy().is_none());
        assert!(off.overwrite_policy().is_none());
        assert!(off.maintenance_policy().is_none());
    }

    #[test]
    fn deletes_and_overwrites_turn_off_independently() {
        let (settings, unknown) = parse("[retention]\nretain-deletes = false\n").unwrap();
        assert!(unknown.is_empty());
        assert!(settings.retention.delete_policy().is_none());
        assert!(settings.retention.overwrite_policy().is_some());
        assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
        let (settings, _) = parse("[retention]\nretain-overwrites = false\n").unwrap();
        assert!(settings.retention.overwrite_policy().is_none());
        assert!(settings.retention.delete_policy().is_some());
        assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
    }

    #[test]
    fn invalid_retention_is_rejected_even_when_disabled() {
        for text in [
            "days = 0",
            "days = 8",
            "max-volume-mib = 0",
            "min-free-mib = -1",
        ] {
            assert!(parse(&format!("[retention]\n{text}\n")).is_err(), "{text}");
        }
    }

    #[test]
    fn policy_parser_ignores_unrelated_types_and_rejects_invalid_policy() {
        let (settings, unknown) = super::super::parse_retention(
            "[automotive]\nmodules = [1]\n[retention]\nretain-deletes = true\n",
        )
        .unwrap();
        assert!(settings.delete_policy().is_some());
        assert!(unknown.is_empty());
        assert!(super::super::parse_retention("[retention]\ndays = 8\n").is_err());
        let (_, unknown) =
            super::super::parse_retention("[retention]\nretain-delete = true\n").unwrap();
        assert_eq!(unknown, ["retention.retain-delete"]);
    }
}
