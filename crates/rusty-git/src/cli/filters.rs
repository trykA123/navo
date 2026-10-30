use std::ffi::OsString;
use std::io::Cursor;

use crate::{BlobReader, FilterNotice, GitError, GitPath, OtherPath, Others, WorktreeBlob};

use super::CliGit;
use super::parse::protocol;

const LFS_MISSING: &str = "git-lfs is not installed; showing the LFS pointer";

pub(super) fn read_worktree_form(
    cli: &CliGit,
    rev: &str,
    path: &GitPath,
) -> Result<WorktreeBlob, GitError> {
    let args = filtered_args(rev, path)?;
    let output = cli.run(&args)?;
    if output.status.success() {
        return Ok(WorktreeBlob {
            reason: filter_reason(cli, path, &output.stdout, &output.stderr)?,
            reader: boxed(output.stdout),
        });
    }
    let raw = cli.run(&raw_blob_args(rev, path)?)?;
    if !raw.status.success() || !is_lfs_pointer(&raw.stdout) || !has_lfs_filter(cli, path)? {
        return Err(cli.command_error(&args, &output));
    }
    Ok(WorktreeBlob {
        reader: boxed(raw.stdout),
        reason: Some(FilterNotice::Lfs {
            detail: lfs_detail(&output.stderr),
        }),
    })
}

fn filtered_args(rev: &str, path: &GitPath) -> Result<Vec<OsString>, GitError> {
    Ok(vec![
        OsString::from("cat-file"),
        OsString::from("--filters"),
        path_argument(path)?,
        OsString::from("--end-of-options"),
        revision_path(rev, path)?,
    ])
}

fn raw_blob_args(rev: &str, path: &GitPath) -> Result<Vec<OsString>, GitError> {
    Ok(vec![
        OsString::from("cat-file"),
        OsString::from("blob"),
        OsString::from("--end-of-options"),
        revision_path(rev, path)?,
    ])
}

fn revision_path(rev: &str, path: &GitPath) -> Result<OsString, GitError> {
    let mut object = Vec::with_capacity(rev.len() + 1 + path.as_bytes().len());
    object.extend_from_slice(rev.as_bytes());
    object.push(b':');
    object.extend_from_slice(path.as_bytes());
    os_string(&object)
}

fn path_argument(path: &GitPath) -> Result<OsString, GitError> {
    let mut argument = Vec::with_capacity(b"--path=".len() + path.as_bytes().len());
    argument.extend_from_slice(b"--path=");
    argument.extend_from_slice(path.as_bytes());
    os_string(&argument)
}

fn filter_reason(
    cli: &CliGit,
    path: &GitPath,
    content: &[u8],
    stderr: &[u8],
) -> Result<Option<FilterNotice>, GitError> {
    let detail = String::from_utf8_lossy(stderr).trim().to_owned();
    if is_lfs_pointer(content) && has_lfs_filter(cli, path)? {
        return Ok(Some(FilterNotice::Lfs {
            detail: lfs_detail(stderr),
        }));
    }
    if detail.is_empty() {
        Ok(None)
    } else {
        Ok(Some(FilterNotice::Filter { detail }))
    }
}

fn lfs_detail(stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_owned();
    if detail.is_empty() {
        LFS_MISSING.to_owned()
    } else {
        detail
    }
}

fn is_lfs_pointer(bytes: &[u8]) -> bool {
    const VERSION: &[u8] = b"version https://git-lfs.github.com/spec/v1\n";
    const OID: &[u8] = b"oid sha256:";
    bytes.starts_with(VERSION) && bytes.windows(OID.len()).any(|window| window == OID)
}

fn has_lfs_filter(cli: &CliGit, path: &GitPath) -> Result<bool, GitError> {
    let args = [
        OsString::from("check-attr"),
        OsString::from("-z"),
        OsString::from("filter"),
        OsString::from("--"),
        os_string(path.as_bytes())?,
    ];
    let output = cli.run(&args)?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .nth(2)
        .is_some_and(|value| value == b"lfs"))
}

fn boxed(bytes: Vec<u8>) -> BlobReader {
    Box::new(Cursor::new(bytes))
}

pub(super) fn read_others(cli: &CliGit, which: Others) -> Result<Vec<OtherPath>, GitError> {
    let args: &[&str] = match which {
        Others::Untracked => &[
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--full-name",
            "--",
        ],
        Others::Ignored => &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "--full-name",
            "--",
        ],
    };
    let output = cli.run_ok(args)?;
    let mut paths = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let (path, is_folder) = match record.strip_suffix(b"/") {
            Some(path) => (path, true),
            None => (record, false),
        };
        if path.is_empty() {
            return Err(protocol());
        }
        paths.push(OtherPath {
            path: GitPath::new(path.to_vec()),
            is_folder,
        });
    }
    Ok(paths)
}

#[cfg(unix)]
fn os_string(bytes: &[u8]) -> Result<OsString, GitError> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
fn os_string(bytes: &[u8]) -> Result<OsString, GitError> {
    let text = std::str::from_utf8(bytes).map_err(|_| GitError::UnrepresentablePath)?;
    Ok(OsString::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unix_arguments_keep_path_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let argument = os_string(&[b'f', 0xFF]).unwrap();
        assert_eq!(argument.as_bytes(), &[b'f', 0xFF]);
    }

    #[cfg(windows)]
    #[test]
    fn unrepresentable_windows_paths_are_typed() {
        assert!(matches!(
            os_string(&[0xFF]),
            Err(GitError::UnrepresentablePath)
        ));
    }
}
