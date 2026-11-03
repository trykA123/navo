use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

const COMMANDS: [&str; 3] = ["clean", "smudge", "process"];
const NESTED: &[u8] = b"/.gitattributes";

/// True when a filter driver with a command is named by the repository's attributes. gix would
/// run it, so the status could not be trusted. A driver nothing refers to is ignored.
pub(super) fn refused(repo: &gix::Repository) -> bool {
    let defined = defined(repo);
    !defined.is_empty() && attribute_files(repo).any(|path| names_one(&path, &defined))
}

/// Lowercase names of the filter drivers that have a command, from any config level.
fn defined(repo: &gix::Repository) -> BTreeSet<String> {
    let snapshot = repo.config_snapshot();
    let Some(sections) = snapshot.plumbing().sections_by_name("filter") else {
        return BTreeSet::new();
    };
    sections
        .filter(|section| COMMANDS.iter().any(|key| section.value(*key).is_some()))
        .filter_map(|section| section.header().subsection_name())
        .map(|name| name.to_string().to_ascii_lowercase())
        .collect()
}

/// The attribute files that apply to the worktree: the root and tracked nested `.gitattributes`,
/// `info/attributes` and the user's attributes file. An untracked nested file is not read.
fn attribute_files(repo: &gix::Repository) -> impl Iterator<Item = PathBuf> {
    let mut files = vec![repo.common_dir().join("info").join("attributes")];
    files.extend(repo.workdir().map(|dir| dir.join(".gitattributes")));
    files.extend(user_file(repo));
    if let Ok(index) = repo.index_or_empty() {
        files.extend(
            index
                .entries()
                .iter()
                .map(|entry| entry.path(&index))
                .filter(|path| path.ends_with(NESTED))
                .filter_map(|path| repo.workdir_path(path)),
        );
    }
    files.into_iter()
}

fn user_file(repo: &gix::Repository) -> Option<PathBuf> {
    match repo.config_snapshot().trusted_path("core.attributesFile") {
        Ok(Some(path)) => Some(path),
        Ok(None) => {
            gix::path::env::xdg_config("git/attributes", &mut |name| std::env::var_os(name))
        }
        Err(_) => None,
    }
}

/// An unreadable file counts as naming a driver, since its content cannot be ruled out.
fn names_one(path: &std::path::Path, defined: &BTreeSet<String>) -> bool {
    match fs::read(path) {
        Ok(bytes) => filter_names(&String::from_utf8_lossy(&bytes))
            .any(|name| defined.contains(&name.to_ascii_lowercase())),
        Err(error) => !matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory),
    }
}

/// Every `filter=<name>` word on a non-comment line, so a macro or a quoted pattern counts too.
fn filter_names(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::split_ascii_whitespace)
        .filter_map(|word| word.strip_prefix("filter="))
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests;
