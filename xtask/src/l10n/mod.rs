mod check;
mod ftl;
mod scan;
mod tokens;

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use check::{ALLOW_FILE, Source, evaluate};

const LOCALES: &str = "crates/rusty-l10n/locales/en-US";
const RATCHET_CRATES: [&str; 2] = ["rusty-ui", "rusty-app"];
const OWN_CRATE: &str = "rusty-l10n";

pub fn run(args: &[String]) -> ExitCode {
    let list = match args {
        [] => false,
        [option] if option == "--list" => true,
        _ => {
            eprintln!("usage: cargo xtask l10n [--list]");
            return ExitCode::from(2);
        }
    };
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("l10n: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let Loaded {
        ftl,
        sources,
        allowed,
    } = match load(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("l10n: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = evaluate(&ftl, &sources, &allowed);
    if list {
        outcome
            .literals
            .iter()
            .for_each(|literal| println!("{literal}"));
    }
    for error in &outcome.errors {
        eprintln!("l10n: {error}");
    }
    if outcome.errors.is_empty() {
        println!(
            "l10n: {} messages checked, {} hard-coded texts (ratchet)",
            outcome.messages,
            outcome.literals.len()
        );
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct Loaded {
    ftl: Vec<(String, String)>,
    sources: Vec<Source>,
    allowed: String,
}

fn load(root: &Path) -> io::Result<Loaded> {
    let mut ftl = Vec::new();
    for path in files_with(&root.join(LOCALES), "ftl")? {
        ftl.push((name_of(&path), fs::read_to_string(&path)?));
    }
    let mut sources = Vec::new();
    let mut crates: Vec<_> = fs::read_dir(root.join("crates"))?.collect::<Result<_, _>>()?;
    crates.sort_by_key(fs::DirEntry::file_name);
    for entry in crates {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == OWN_CRATE {
            continue;
        }
        let src = entry.path().join("src");
        if !src.is_dir() {
            continue;
        }
        for path in files_with(&src, "rs")? {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if is_test_path(relative) {
                continue;
            }
            sources.push(Source {
                path: relative.display().to_string(),
                text: fs::read_to_string(&path)?,
                in_ratchet: RATCHET_CRATES.contains(&name.as_str()),
            });
        }
    }
    Ok(Loaded {
        ftl,
        sources,
        allowed: fs::read_to_string(root.join(ALLOW_FILE))?,
    })
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn files_with(dir: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn is_test_path(path: &Path) -> bool {
    path.iter().any(|part| {
        let part = part.to_string_lossy();
        let stem = part.strip_suffix(".rs").unwrap_or(&part);
        stem == "tests"
            || stem == "test"
            || stem.starts_with("test_")
            || stem.starts_with("tests_")
            || stem.ends_with("_tests")
            || stem.ends_with("_test")
    })
}
