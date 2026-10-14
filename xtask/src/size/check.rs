use std::path::{Path, PathBuf};

use super::{DEFAULT_LIMIT, Exception, SizeViolation, count_code_lines};

pub fn check_files(files: &[(PathBuf, String)], exceptions: &[Exception]) -> Vec<SizeViolation> {
    files
        .iter()
        .filter(|(path, _)| !is_test_file(path))
        .filter_map(|(path, source)| {
            let limit = exceptions
                .iter()
                .find(|exception| exception.path == *path)
                .map_or(DEFAULT_LIMIT, |exception| exception.limit);
            let lines = count_code_lines(source);
            (lines > limit).then(|| SizeViolation {
                path: path.clone(),
                lines,
                limit,
            })
        })
        .collect()
}

pub(super) fn is_test_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}
