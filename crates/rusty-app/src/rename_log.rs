use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusty_core::{Location, OpKind, OperationPlan, Step};
use rusty_ui::browser::{RenameLogStatus, WriteRenameLog};

mod rotation;

const FORMAT: &str = "# navo rename log format 1\n";
const LEGACY_FORMAT: &str = "# ferrite rename log format 1\n";

pub fn writer(path: Option<PathBuf>) -> WriteRenameLog {
    Arc::new(move |plan, status| {
        let path = path
            .as_deref()
            .ok_or_else(|| "the profile folder is not available".to_owned())?;
        append(path, plan, status).map_err(|error| format!("writing {}: {error}", path.display()))
    })
}

fn pairs(
    plan: &OperationPlan,
    status: RenameLogStatus,
) -> Result<Vec<(Location, Location)>, String> {
    if plan.kind != OpKind::Rename {
        return Err("not a rename batch".to_owned());
    }
    let mut positions = HashMap::new();
    let mut pairs: Vec<(Location, Location)> = Vec::new();
    for step in &plan.steps {
        let Step::Rename { source, target } = step else {
            return Err("not a rename step".to_owned());
        };
        let index = positions.remove(source).unwrap_or_else(|| {
            pairs.push((source.clone(), source.clone()));
            pairs.len() - 1
        });
        pairs[index].1 = target.clone();
        positions.insert(target.clone(), index);
    }
    if status == RenameLogStatus::Complete
        && pairs
            .iter()
            .any(|(_, target)| rusty_fs::is_rename_temporary(target))
    {
        return Err("unfinished rename staging".to_owned());
    }
    pairs.retain(|(source, target)| source != target);
    Ok(pairs)
}

fn append(path: &Path, plan: &OperationPlan, status: RenameLogStatus) -> Result<(), String> {
    let pairs = pairs(plan, status)?;
    if pairs.is_empty() {
        return Ok(());
    }
    let state = if status == RenameLogStatus::Incomplete {
        "incomplete rename batch"
    } else {
        "rename batch"
    };
    let mut batch = format!(
        "{FORMAT}# {state} {} UTC milliseconds\n",
        crate::files::now_ms()
    );
    for (source, target) in pairs {
        let source = serde_json::to_string(source.as_str()).map_err(|error| error.to_string())?;
        let target = serde_json::to_string(target.as_str()).map_err(|error| error.to_string())?;
        batch.push_str(&format!("{source} → {target}\n"));
        if batch.len() as u64 > rotation::MAX_BYTES {
            return Err("rename log batch exceeds the size limit".to_owned());
        }
    }
    rotation::append(path, batch.as_bytes())
}

#[cfg(test)]
mod tests;
