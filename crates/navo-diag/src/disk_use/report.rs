use std::fmt::Write;
use std::path::Path;

use serde::Serialize;

use super::{DiskKind, DiskState, DiskTotals, DiskUse};
use crate::{DriveKind, PathKind, Redactor};

#[derive(Serialize)]
pub(crate) struct DiskExport {
    total: DiskTotals,
    locations: Vec<DiskLocationExport>,
}

#[derive(Serialize)]
struct DiskLocationExport {
    kind: DiskKind,
    path: String,
    parts: Vec<DiskPartExport>,
    #[serde(flatten)]
    usage: DiskTotals,
}

#[derive(Serialize)]
struct DiskPartExport {
    path: String,
    #[serde(flatten)]
    usage: DiskTotals,
}

pub(crate) struct DiskReport {
    pub(crate) export: DiskExport,
    pub(crate) summary: String,
}

impl DiskUse {
    pub(crate) fn report(&self, redactor: &Redactor) -> DiskReport {
        let locations = self
            .locations
            .iter()
            .map(|location| DiskLocationExport {
                kind: location.kind,
                path: redacted_path(redactor, &location.path, location.path_kind),
                parts: location
                    .parts
                    .iter()
                    .map(|part| DiskPartExport {
                        path: redacted_path(redactor, &part.path, PathKind::File),
                        usage: part.usage.clone(),
                    })
                    .collect(),
                usage: location.usage.clone(),
            })
            .collect();
        let export = DiskExport {
            total: self.total.clone(),
            locations,
        };
        let summary = summary(&export);
        DiskReport { export, summary }
    }
}

fn redacted_path(redactor: &Redactor, path: &Path, kind: PathKind) -> String {
    redactor
        .path(&path.to_string_lossy(), kind, DriveKind::Unknown)
        .to_string()
}

fn summary(disk: &DiskExport) -> String {
    let mut text = String::from("\nDisk use:\n");
    for location in &disk.locations {
        let _ = writeln!(
            text,
            "  {}: {}: {}{} bytes, {} files",
            location.kind.label(),
            location.path,
            lower_bound(&location.usage),
            location.usage.bytes,
            location.usage.files,
        );
    }
    let _ = writeln!(
        text,
        "Total: {}{} bytes, {} files",
        lower_bound(&disk.total),
        disk.total.bytes,
        disk.total.files,
    );
    text
}

fn lower_bound(usage: &DiskTotals) -> &'static str {
    if usage.state == DiskState::AtLeast {
        "at least "
    } else {
        ""
    }
}
