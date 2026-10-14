use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::ExitCode,
};

use super::{Exception, check::is_test_file, check_files, parse_exceptions};

pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask size");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("size: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    match run_check(root) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

pub(super) fn run_check(root: &Path) -> Result<bool, String> {
    let exceptions = read_exceptions(root)?;
    let files = read_files(root)?;
    let violations = check_files(&files, &exceptions);
    for violation in &violations {
        println!(
            "{}: {} > {}",
            violation.path.to_string_lossy().replace('\\', "/"),
            violation.lines,
            violation.limit
        );
    }
    if violations.is_empty() {
        println!("size: {} files checked", files.len());
    }
    Ok(violations.is_empty())
}

fn read_exceptions(root: &Path) -> Result<Vec<Exception>, String> {
    let path = root.join("size-exceptions.toml");
    match fs::read_to_string(&path) {
        Ok(source) => parse_exceptions(&source).map_err(|error| error.to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub(super) fn read_files(root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut files = Vec::new();
    for directory in ["crates", "xtask"] {
        visit_directory(root, &root.join(directory), &mut files)?;
    }
    files.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(files)
}

fn visit_directory(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        visit_entry(root, entry, files)?;
    }
    Ok(())
}

fn visit_entry(
    root: &Path,
    entry: fs::DirEntry,
    files: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let path = entry.path();
    let file_type = entry
        .file_type()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if file_type.is_dir() {
        if entry.file_name() != "target" {
            visit_directory(root, &path, files)?;
        }
        return Ok(());
    }
    if !file_type.is_file() || path.extension().is_none_or(|extension| extension != "rs") {
        return Ok(());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if is_test_file(relative) {
        return Ok(());
    }
    let source =
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    files.push((relative.to_path_buf(), source));
    Ok(())
}
