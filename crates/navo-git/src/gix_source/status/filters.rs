use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

const COMMANDS: [&str; 3] = ["clean", "smudge", "process"];
const NESTED: &[u8] = b"/.gitattributes";
const LFS: &str = "lfs";

/// What the repository's attributes ask of filter drivers.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Filters {
    /// No attribute names a driver that has a command.
    Clear,
    /// Only the standard git-lfs driver is named; its pointers are computed natively.
    Lfs,
    /// A driver gix would run is named, or the git-lfs driver is not the standard one.
    Foreign(String),
}

struct Driver {
    name: String,
    commands: Vec<String>,
}

impl Driver {
    fn is_standard_lfs(&self) -> bool {
        self.name == LFS
            && self
                .commands
                .iter()
                .all(|command| command == "git-lfs" || command.starts_with("git-lfs "))
    }
}

/// Classifies the filter drivers named by the repository's attributes. A driver nothing refers
/// to is ignored; an unreadable attributes file counts as naming the first driver.
pub(super) fn classify(repo: &gix::Repository) -> Filters {
    let drivers = defined(repo);
    let Some(first) = drivers.first() else {
        return Filters::Clear;
    };
    let mut lfs = false;
    for path in attribute_files(repo) {
        let text = match fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                continue;
            }
            Err(_) => return Filters::Foreign(first.name.clone()),
        };
        for name in filter_names(&text) {
            let mut matching = drivers
                .iter()
                .filter(|driver| driver.name.eq_ignore_ascii_case(name))
                .peekable();
            if matching.peek().is_none() {
                continue;
            }
            if name == LFS && matching.all(Driver::is_standard_lfs) {
                lfs = true;
            } else {
                return Filters::Foreign(name.to_owned());
            }
        }
    }
    if lfs { Filters::Lfs } else { Filters::Clear }
}

/// The drivers that have a command, from any config level, with the commands of every section.
fn defined(repo: &gix::Repository) -> Vec<Driver> {
    let snapshot = repo.config_snapshot();
    let Some(sections) = snapshot.plumbing().sections_by_name("filter") else {
        return Vec::new();
    };
    let mut drivers: Vec<Driver> = Vec::new();
    for section in sections {
        let commands: Vec<String> = COMMANDS
            .iter()
            .filter_map(|key| section.value(*key))
            .map(|value| value.to_string())
            .collect();
        let Some(name) = section.header().subsection_name() else {
            continue;
        };
        if commands.is_empty() {
            continue;
        }
        let name = name.to_string();
        match drivers.iter_mut().find(|driver| driver.name == name) {
            Some(driver) => driver.commands.extend(commands),
            None => drivers.push(Driver { name, commands }),
        }
    }
    drivers
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
