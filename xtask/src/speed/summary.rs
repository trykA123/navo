//! Read only benchmark identity and total Callgrind instruction counts.

use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Measured {
    pub id: String,
    pub new_ir: u64,
    pub old_ir: Option<u64>,
}

/// Parse one summary into instruction counts; fail on missing head Ir or invalid JSON.
pub fn measured(json: &str) -> Result<Measured, String> {
    let summary: Summary = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let package = summary
        .package_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("summary has no package directory name")?;
    let mut profiles = summary
        .profiles
        .iter()
        .filter(|profile| profile.tool == "Callgrind");
    let values = profiles
        .next()
        .and_then(|profile| profile.data.total.metrics.ir.as_ref())
        .ok_or("summary has no head Ir in Callgrind total")?;
    if profiles.next().is_some() {
        return Err("summary has multiple Callgrind profiles".into());
    }
    let new_ir = values.values.new;
    let suffix = summary.id.map(|id| format!(".{id}")).unwrap_or_default();
    Ok(Measured {
        id: format!("{package}/{}{suffix}", summary.module_path),
        new_ir,
        old_ir: values.values.old,
    })
}

/// Read every nested summary.json; fail on unreadable inputs or an empty directory.
pub fn collect(dir: &Path) -> Result<Vec<Measured>, String> {
    let mut directories = vec![dir.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in super::read_directory(&directory)? {
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file() && entry.file_name() == "summary.json" {
                files.push(path);
            }
        }
    }
    if files.is_empty() {
        return Err(format!("no summary.json files under {}", dir.display()));
    }
    files.sort();
    files
        .iter()
        .map(|path| {
            let json = fs::read_to_string(path)
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            measured(&json).map_err(|error| format!("parsing {}: {error}", path.display()))
        })
        .collect()
}

#[derive(Deserialize)]
struct Summary {
    package_dir: PathBuf,
    module_path: String,
    id: Option<String>,
    profiles: Vec<Profile>,
}

#[derive(Deserialize)]
struct Profile {
    tool: String,
    data: ProfileData,
}

#[derive(Deserialize)]
struct ProfileData {
    total: Total,
}

#[derive(Deserialize)]
struct Total {
    metrics: Metrics,
}

#[derive(Deserialize)]
struct Metrics {
    #[serde(rename = "Ir")]
    ir: Option<Metric>,
}

#[derive(Deserialize)]
struct Metric {
    values: Values,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Values {
    new: u64,
    #[serde(default, deserialize_with = "present_old")]
    old: Option<u64>,
}

/// Read a present old Ir as an integer; reject null while allowing an absent field.
fn present_old<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speed::tests::TestTree;
    use serde_json::json;

    /// Build schema-7 JSON using the runner's flat metrics and new/old serde fields.
    fn fixture(values: serde_json::Value) -> serde_json::Value {
        json!({
            "package_dir": "crates/navo-fs",
            "module_path": "listing::listing::bench_sort_order",
            "id": "name",
            "profiles": [{"tool": "Callgrind", "data": {
                "parts": [{"metrics": {"Ir": {"values": {"new": 9999}}}}],
                "total": {"metrics": {"Ir": {"values": values}}}
            }}]
        })
    }

    #[test]
    fn parses_total_new_and_old_ir_from_runner_schema() -> Result<(), String> {
        let reading = measured(&fixture(json!({"new": 1051, "old": 1000})).to_string())?;
        assert_eq!(
            reading.id,
            "navo-fs/listing::listing::bench_sort_order.name"
        );
        assert_eq!(reading.new_ir, 1051);
        assert_eq!(reading.old_ir, Some(1000));
        Ok(())
    }

    #[test]
    fn parses_new_only_without_optional_id() -> Result<(), String> {
        let mut input = fixture(json!({"new": 1000}));
        input.as_object_mut().ok_or("not an object")?.remove("id");
        let reading = measured(&input.to_string())?;
        assert_eq!(reading.id, "navo-fs/listing::listing::bench_sort_order");
        assert_eq!(reading.old_ir, None);
        Ok(())
    }

    #[test]
    fn rejects_missing_head_ir_including_old_only_and_other_tools() {
        let mut no_ir = fixture(json!({"new": 1000}));
        no_ir["profiles"][0]["data"]["total"]["metrics"] = json!({});
        let mut other_tool = fixture(json!({"new": 1000}));
        other_tool["profiles"][0]["tool"] = json!("Cachegrind");
        let mut no_profile = fixture(json!({"new": 1000}));
        no_profile["profiles"] = json!([]);
        for input in [fixture(json!({"old": 1000})), no_ir, other_tool, no_profile] {
            assert!(measured(&input.to_string()).is_err(), "{input}");
        }
    }

    #[test]
    fn rejects_malformed_json_and_invalid_counts() {
        assert!(measured("{").is_err());
        for values in [
            json!({"new": null}),
            json!({"new": -1}),
            json!({"new": "NaN"}),
        ] {
            assert!(measured(&fixture(values).to_string()).is_err());
        }
    }

    #[test]
    fn rejects_unknown_old_key() {
        assert!(measured(&fixture(json!({"new": 100, "old_typo": 90})).to_string()).is_err());
    }

    #[test]
    fn rejects_explicit_null_old() {
        assert!(measured(&fixture(json!({"new": 100, "old": null})).to_string()).is_err());
    }

    #[test]
    fn collects_nested_summaries_and_rejects_empty_or_malformed_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        assert!(collect(&tree.0).is_err());
        std::fs::create_dir_all(tree.0.join("a/b"))?;
        let source = fixture(json!({"new": 1000})).to_string();
        std::fs::write(tree.0.join("summary.json"), &source)?;
        std::fs::write(tree.0.join("a/b/summary.json"), &source)?;
        std::fs::write(tree.0.join("ignored.json"), "invalid")?;
        assert_eq!(collect(&tree.0)?.len(), 2);
        std::fs::write(tree.0.join("a/b/summary.json"), "invalid")?;
        let error = collect(&tree.0).err().ok_or("invalid summary accepted")?;
        assert!(error.contains("a/b/summary.json"), "{error}");
        Ok(())
    }
}
