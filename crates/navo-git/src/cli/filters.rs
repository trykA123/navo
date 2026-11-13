use std::ffi::OsString;
use std::io::{self, Cursor, Read};

use crate::{BlobReader, FilterNotice, GitError, GitPath, OtherPath, Others, WorktreeBlob};

use super::CliGit;
use super::filtered_stream::FilteredStream;
use super::parse::protocol;
use super::process::CommandSpec;

const LFS_MISSING: &str = "git-lfs is not installed; showing the LFS pointer";

pub(super) fn read_worktree_form(
    cli: &CliGit,
    rev: &str,
    path: &GitPath,
) -> Result<WorktreeBlob, GitError> {
    if cli.raw_odb_only {
        return read_raw_odb(cli, rev, path);
    }
    let args = filtered_args(rev, path)?;
    let spec = CommandSpec {
        git: &cli.git,
        repo: Some(&cli.repo),
        git_dir: None,
        env: &cli.env,
        timeout: cli.timeout,
    };
    let mut stream = FilteredStream::start(&spec, &args)?;
    let prefix = match read_prefix(&mut stream) {
        Ok(prefix) => prefix,
        Err(error) => return failed_read(cli, (rev, path), &stream, error),
    };
    let reason = filter_reason(cli, path, &prefix)?;
    let finish = Some(stream.finisher());
    Ok(WorktreeBlob {
        reader: Box::new(Cursor::new(prefix).chain(stream)),
        reason,
        finish,
    })
}

/// Reads raw ODB blob bytes directly, bypassing `.gitattributes`, filters, and lazy fetch.
fn read_raw_odb(cli: &CliGit, rev: &str, path: &GitPath) -> Result<WorktreeBlob, GitError> {
    let output = cli.run_ok(&raw_blob_args(rev, path)?)?;
    Ok(WorktreeBlob {
        reader: boxed(output.stdout),
        reason: None,
        finish: None,
    })
}

fn failed_read(
    cli: &CliGit,
    (rev, path): (&str, &GitPath),
    stream: &FilteredStream,
    error: io::Error,
) -> Result<WorktreeBlob, GitError> {
    let output = match stream.output() {
        Some(Ok(output)) if !output.status.success() => output,
        Some(Err(error)) => return Err(error),
        _ => {
            if let Some(inner) = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<GitError>())
            {
                return Err(inner.clone());
            }
            return Err(GitError::Io {
                path: cli.git.clone(),
                kind: error.kind(),
            });
        }
    };
    let raw = cli.run(&raw_blob_args(rev, path)?)?;
    if !raw.status.success() || !is_lfs_pointer(&raw.stdout) || !has_lfs_filter(cli, path)? {
        return Err(cli.command_error(&filtered_args(rev, path)?, &output));
    }
    Ok(WorktreeBlob {
        reader: boxed(raw.stdout),
        reason: Some(FilterNotice::Lfs {
            detail: lfs_detail(&output.stderr),
        }),
        finish: None,
    })
}

fn read_prefix(stream: &mut FilteredStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(1023);
    let mut chunk = [0; 1023];
    loop {
        let read = match stream.read(&mut chunk[..1023 - bytes.len()]) {
            Err(_) if !bytes.is_empty() => return Ok(bytes),
            result => result?,
        };
        bytes.extend_from_slice(&chunk[..read]);
        let version = b"version https://git-lfs.github.com/spec/v1\n";
        if read == 0
            || bytes.len() == 1023
            || is_lfs_pointer(&bytes)
            || !bytes.iter().zip(version).all(|(left, right)| left == right)
        {
            return Ok(bytes);
        }
    }
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
) -> Result<Option<FilterNotice>, GitError> {
    if is_lfs_pointer(content) && has_lfs_filter(cli, path)? {
        return Ok(Some(FilterNotice::Lfs {
            detail: LFS_MISSING.to_owned(),
        }));
    }
    Ok(None)
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
