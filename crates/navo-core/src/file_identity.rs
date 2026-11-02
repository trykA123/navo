#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileIdentityKind {
    #[default]
    Native,
    WindowsIndex,
    Fingerprint {
        folder: bool,
    },
}

impl FileIdentityKind {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "Serde skip_serializing_if requires a borrowed callback argument"
    )]
    fn is_native(&self) -> bool {
        *self == Self::Native
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub volume: u64,
    pub file: [u8; 16],
    #[serde(default, skip_serializing_if = "FileIdentityKind::is_native")]
    pub kind: FileIdentityKind,
}

const DST_SHIFT_MS: u64 = 3_600_000;

impl FileIdentity {
    #[must_use]
    pub fn fingerprint(size: u64, modified_ms: Option<i64>, folder: bool) -> Self {
        let mut file = [0; 16];
        file[..8].copy_from_slice(&size.to_le_bytes());
        file[8..].copy_from_slice(&modified_ms.unwrap_or(i64::MIN).to_le_bytes());
        Self {
            volume: 0,
            kind: FileIdentityKind::Fingerprint { folder },
            file,
        }
    }

    #[must_use]
    pub fn same_fingerprint(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        let same_shape = matches!(self.kind, FileIdentityKind::Fingerprint { .. })
            && self.kind == other.kind
            && self.file[..8] == other.file[..8];
        same_shape
            && match (self.modified_ms(), other.modified_ms()) {
                (Some(left), Some(right)) => left.abs_diff(right) == DST_SHIFT_MS,
                _ => false,
            }
    }

    fn modified_ms(&self) -> Option<i64> {
        let mut time = [0; 8];
        time.copy_from_slice(&self.file[8..]);
        Some(i64::from_le_bytes(time)).filter(|modified| *modified != i64::MIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_round_trip_without_becoming_native_or_legacy_identity() {
        for folder in [false, true] {
            let identity = FileIdentity::fingerprint(12, Some(3_600_000), folder);
            let bytes = serde_json::to_vec(&identity).unwrap();
            let decoded: FileIdentity = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded, identity);
            assert_eq!(decoded.kind, FileIdentityKind::Fingerprint { folder });
            for kind in [FileIdentityKind::Native, FileIdentityKind::WindowsIndex] {
                assert!(!decoded.same_fingerprint(&FileIdentity { kind, ..decoded }));
            }
            assert!(!decoded.same_fingerprint(&FileIdentity::fingerprint(
                12,
                Some(3_600_000),
                !folder
            )));
        }
    }

    #[test]
    fn old_native_json_stays_compatible_and_legacy_domains_survive_serialization() {
        let old = serde_json::json!({"volume": 7, "file": vec![1; 16]});
        let mut identity: FileIdentity = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(identity.kind, FileIdentityKind::Native);
        assert_eq!(serde_json::to_value(identity).unwrap(), old);
        identity.kind = FileIdentityKind::WindowsIndex;
        let encoded = serde_json::to_vec(&identity).unwrap();
        assert_eq!(
            serde_json::from_slice::<FileIdentity>(&encoded).unwrap(),
            identity
        );
        assert_ne!(
            identity,
            FileIdentity {
                kind: FileIdentityKind::Native,
                ..identity
            }
        );
    }
}
