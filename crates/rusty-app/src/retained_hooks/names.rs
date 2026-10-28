use rusty_core::retention::{RetainedKind, RetentionEntry};
use rusty_core::{FsErrorKind, Location, ProviderPath};
use rusty_fs::ProviderOps;

const MAX_RESTORED_NAMES: u32 = 100;

pub(super) fn free_target(
    ops: &dyn ProviderOps,
    entry: &RetentionEntry,
) -> Result<Location, String> {
    let original = &entry.record.source;
    if !exists(ops, original, entry)? {
        return Ok(original.clone());
    }
    let (Some(parent), Some(name)) = original.parent_and_name() else {
        return Err(rusty_l10n::t!("retained-name-taken", name = shown(entry)));
    };
    let is_folder = entry
        .record
        .nodes
        .first()
        .is_some_and(|node| node.kind == RetainedKind::Folder);
    for number in 1..=MAX_RESTORED_NAMES {
        let candidate = restored_name(&name.to_text(), number, is_folder);
        let Ok(target) = parent.join(&ProviderPath::from_text(&candidate)) else {
            continue;
        };
        if !exists(ops, &target, entry)? {
            return Ok(target);
        }
    }
    Err(rusty_l10n::t!("retained-name-taken", name = shown(entry)))
}

pub(super) fn shown(entry: &RetentionEntry) -> String {
    entry.record.source.name().map_or_else(
        || entry.record.source.as_str().to_owned(),
        |name| name.to_text(),
    )
}

fn restored_name(name: &str, number: u32, is_folder: bool) -> String {
    let tag = if number == 1 {
        " (restored)".to_owned()
    } else {
        format!(" (restored {number})")
    };
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 && !is_folder => name.split_at(dot),
        _ => (name, ""),
    };
    format!("{stem}{tag}{extension}")
}

fn exists(ops: &dyn ProviderOps, at: &Location, entry: &RetentionEntry) -> Result<bool, String> {
    match ops.metadata(at) {
        Ok(_) => Ok(true),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(false),
        Err(error) => Err(rusty_l10n::t!(
            "retained-restore-failed",
            name = shown(entry),
            error = format!("{:?}", error.kind)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::restored_name;

    #[test]
    fn restored_names_go_before_the_last_extension_of_files_only() {
        assert_eq!(
            restored_name("report.txt", 1, false),
            "report (restored).txt"
        );
        assert_eq!(restored_name("a.tar.gz", 2, false), "a.tar (restored 2).gz");
        assert_eq!(restored_name(".bashrc", 1, false), ".bashrc (restored)");
        assert_eq!(restored_name("notes", 1, false), "notes (restored)");
        assert_eq!(restored_name("v1.2", 1, true), "v1.2 (restored)");
    }
}
