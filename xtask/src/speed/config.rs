//! Strict TOML schemas for speed manifests, exceptions and retirements.

use serde::Deserialize;
use std::{fs, io::ErrorKind, path::Path};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, rename = "bench")]
    pub benches: Vec<BenchEntry>,
    pub recovery_base: Option<String>,
    pub recovery_reason: Option<String>,
    #[serde(default)]
    pub accepted: Vec<Accepted>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchEntry {
    pub id: String,
    pub guarded: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub bench: String,
    pub base: String,
    pub max_percent: f64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    pub id: String,
    pub base: String,
    pub reason: String,
}

/// Load a speed manifest from a path, returning read or schema errors.
pub fn load_config(path: &Path) -> Result<Config, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
    toml::from_str(&source).map_err(|error| format!("parsing {}: {error}", path.display()))
}

/// Load retirements from a path; missing files mean no retirements.
pub fn load_retired(path: &Path) -> Result<Vec<Retired>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Retirements {
        #[serde(default)]
        retired: Vec<Retired>,
    }
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("reading {}: {error}", path.display())),
    };
    toml::from_str::<Retirements>(&source)
        .map(|config| config.retired)
        .map_err(|error| format!("parsing {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speed::tests::TestTree;

    #[test]
    fn full_config_parses() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        let path = tree.0.join("speed.toml");
        std::fs::write(
            &path,
            r#"
recovery_base = "abc"
recovery_reason = "repair"
[[bench]]
id = "navo-fs/listing.sort"
guarded = true
[[accepted]]
bench = "navo-fs/listing.sort"
base = "v0.5.0"
max_percent = 8.0
reason = "extra validation"
"#,
        )?;
        let config = load_config(&path)?;
        assert_eq!(config.benches[0].id, "navo-fs/listing.sort");
        assert!(config.benches[0].guarded);
        assert_eq!(config.recovery_base.as_deref(), Some("abc"));
        assert_eq!(config.recovery_reason.as_deref(), Some("repair"));
        let accepted = &config.accepted[0];
        assert_eq!(accepted.bench, "navo-fs/listing.sort");
        assert_eq!(accepted.base, "v0.5.0");
        assert_eq!(accepted.max_percent, 8.0);
        assert_eq!(accepted.reason, "extra validation");
        Ok(())
    }

    #[test]
    fn unknown_key_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        let path = tree.0.join("speed.toml");
        for input in [
            "unexpected = true",
            "[[bench]]\nid = 'x'\nguarded = true\nunexpected = true",
            "[[accepted]]\nbench = 'x'\nbase = 'v1'\nmax_percent = 6.0\nreason = 'why'\nunexpected = true",
        ] {
            std::fs::write(&path, input)?;
            let error = load_config(&path).err().ok_or("unknown key accepted")?;
            assert!(error.contains("unknown field"), "{error}");
        }
        Ok(())
    }

    #[test]
    fn missing_retired_file_gives_empty_list() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        assert!(load_retired(&tree.0.join("missing.toml"))?.is_empty());
        Ok(())
    }
}
