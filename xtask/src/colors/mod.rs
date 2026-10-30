mod check;
mod scan;

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use crate::l10n::{files_with, is_test_path};
use check::{ALLOW_FILE, Kind, Source, evaluate};

const ROOTS: [&str; 2] = ["crates", "assets"];

pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask colors");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("colors: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let (sources, allow) = match load(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("colors: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = evaluate(&sources, &allow);
    for error in &outcome.errors {
        eprintln!("colors: {error}");
    }
    if outcome.errors.is_empty() {
        println!(
            "colors: {} files checked, no colour outside the token layer",
            outcome.files
        );
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn load(root: &Path) -> io::Result<(Vec<Source>, String)> {
    let mut sources = Vec::new();
    for folder in ROOTS {
        let dir = root.join(folder);
        if dir.is_dir() {
            read_folder(root, &dir, &mut sources)?;
        }
    }
    Ok((sources, fs::read_to_string(root.join(ALLOW_FILE))?))
}

fn read_folder(root: &Path, dir: &Path, sources: &mut Vec<Source>) -> io::Result<()> {
    for (extension, kind) in [("rs", Kind::Rust), ("svg", Kind::Svg)] {
        let paths = files_with(dir, extension)?;
        for path in paths {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if is_test_path(relative) {
                continue;
            }
            sources.push(Source {
                path: relative.to_string_lossy().replace('\\', "/"),
                text: fs::read_to_string(&path)?,
                kind,
            });
        }
    }
    Ok(())
}
