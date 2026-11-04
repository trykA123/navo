use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const MARKER: &str = "zq7-marker-xk4";
pub(crate) const RARE_EXT: &str = "nbq";
pub(crate) const PRIVATE_FOLDER: &str = "zqxjv-private&folder";
pub(crate) const PRIVATE_FILE: &str = "zqxjv-private-file";
pub(crate) const PRIVATE_EXTENSION: &str = "zqxjvextension";
pub(crate) const PRIVATE_QUERY: &str = "zqxjv-query-prefix";
pub(crate) const PRIVATE_CONTENT: &str = "zqxjv-private-content";
pub(crate) const CONTENT: &[u8] = b"planted fixture bytes";
pub(crate) const TARGET_A: &str = "retarget-alpha.txt";
pub(crate) const TARGET_B: &str = "retarget-bravo.txt";
pub(crate) const KIND_DIR: &str = "profile-kinds";
pub(crate) const KIND_TOOL: &str = "noext-tool";
pub(crate) const REPO_DIR: &str = "profile-repo";
pub(crate) const FOLD_EXT: &str = "cfgx";
pub(crate) const LINK_NAME: &str = "zq7-inner-link";

pub(crate) struct Fixture {
    pub(crate) dir: tempfile::TempDir,
    pub(crate) marker_file: PathBuf,
    pub(crate) rare_file: PathBuf,
    pub(crate) hidden_file: PathBuf,
    pub(crate) readonly_file: PathBuf,
    pub(crate) hard_link: PathBuf,
    pub(crate) long_file: PathBuf,
    pub(crate) alias_source: PathBuf,
    pub(crate) outside: tempfile::TempDir,
    pub(crate) link_out: Option<PathBuf>,
    pub(crate) privacy_file: PathBuf,
    pub(crate) unreadable_dir: PathBuf,
    /// Relative link to `TARGET_A`; the two targets have equal-length names.
    #[cfg_attr(
        windows,
        allow(dead_code, reason = "only the unix manifest test reads it")
    )]
    pub(crate) inner_link: Option<PathBuf>,
}

impl Fixture {
    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    #[cfg(unix)]
    #[allow(
        clippy::disallowed_methods,
        reason = "the fixture toggles permissions on its own temp tree"
    )]
    pub(crate) fn with_unreadable_directory<T>(&self, run: impl FnOnce() -> T) -> io::Result<T> {
        use std::os::unix::fs::PermissionsExt;
        let original = fs::metadata(&self.unreadable_dir)?.permissions();
        fs::set_permissions(&self.unreadable_dir, fs::Permissions::from_mode(0o000))?;
        let result = run();
        fs::set_permissions(&self.unreadable_dir, original)?;
        Ok(result)
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture owns both isolated temp trees"
)]
pub(crate) fn set() -> io::Result<Fixture> {
    let dir = tempdir()?;
    let outside = tempdir()?;
    let root = dir.path();
    let marker_file = write(&root.join(format!("{MARKER}-report.txt")))?;
    fs::write(&marker_file, CONTENT)?;
    let rare_file = write(&root.join(format!("sample.{RARE_EXT}")))?;
    let hidden_file = write(&root.join(".hidden-entry"))?;
    set_hidden(&hidden_file)?;
    let readonly_file = write(&root.join("readonly.txt"))?;
    let mut perms = fs::metadata(&readonly_file)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(&readonly_file, perms)?;
    let hard_link = root.join("hard-link.txt");
    fs::hard_link(&marker_file, &hard_link)?;
    let long_file = write(&long_path(root))?;
    let alias_source = write(&root.join("LongAliasSourceName.markdown"))?;
    write(&outside.path().join("outside.txt"))?;
    let link_out = link_out(root, outside.path());
    let privacy_file = write(
        &root
            .join(PRIVATE_FOLDER)
            .join(format!("{PRIVATE_FILE}.{PRIVATE_EXTENSION}")),
    )?;
    fs::write(&privacy_file, format!("{PRIVATE_CONTENT}\n{PRIVATE_QUERY}"))?;
    let unreadable_dir = root.join("unreadable");
    write(&unreadable_dir.join("inside.txt"))?;
    write(&root.join(TARGET_A))?;
    write(&root.join(TARGET_B))?;
    let inner_link = inner_link(root);
    plant_profile(root)?;
    Ok(Fixture {
        dir,
        marker_file,
        rare_file,
        hidden_file,
        readonly_file,
        hard_link,
        long_file,
        alias_source,
        outside,
        link_out,
        privacy_file,
        unreadable_dir,
        inner_link,
    })
}

/// A fresh temp folder whose path the report cannot echo by chance. Every component of a
/// measured folder's path is protected, and Windows `%TEMP%` ends in `Temp`, which the report
/// text holds inside `attempted` and `grid-template`; `%LOCALAPPDATA%` is its parent.
pub(crate) fn tempdir() -> io::Result<tempfile::TempDir> {
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return tempfile::Builder::new().prefix(".nbt").tempdir_in(base);
    }
    tempfile::tempdir()
}

/// Writes a stand-in navo exe: `padding` filler bytes, the tester marker if asked, then a tail.
#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture creates only its own planted entries"
)]
pub(crate) fn stand_in_exe(folder: &Path, padding: usize, marker: bool) -> io::Result<()> {
    let mut bytes = vec![b'x'; padding];
    if marker {
        bytes.extend_from_slice(crate::harness::probe::MARKER);
    }
    bytes.extend_from_slice(b"tail");
    let name = if cfg!(windows) { "navo.exe" } else { "navo" };
    fs::write(folder.join(name), bytes)
}

#[allow(
    clippy::disallowed_methods,
    reason = "the fixture plants kind, repository and extension samples in its own temp tree"
)]
fn plant_profile(root: &Path) -> io::Result<()> {
    let kinds = root.join(KIND_DIR);
    let samples: [(&str, &[u8]); 9] = [
        ("pack.zip", b"PK\x03\x04body"),
        ("pic.png", b"\x89PNG\r\n\x1a\n"),
        ("doc.pdf", b"%PDF-1.7"),
        (KIND_TOOL, b"\x7fELF\x02\x01\x01\0"),
        ("firmware.map", b"Memory map"),
        ("cal.a2l", b"ASAP2_VERSION"),
        ("bus.dbc", b"VERSION"),
        ("plain.txt", b"plain words"),
        ("blob.dat", &[1, 0, 2, 0, 3, 0]),
    ];
    for (name, bytes) in samples {
        fs::write(write(&kinds.join(name))?, bytes)?;
    }
    fs::write(
        write(&root.join(REPO_DIR).join(".git").join("HEAD"))?,
        b"ref: main",
    )?;
    fs::write(write(&root.join(REPO_DIR).join("code.txt"))?, [b'x'; 100])?;
    for folder in ["fold-a", "fold-b"] {
        for index in 0..6 {
            write(&root.join(folder).join(format!("f{index}.{FOLD_EXT}")))?;
        }
    }
    Ok(())
}

#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture creates only its own planted entries"
)]
fn write(path: &Path) -> io::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(path)?;
    Ok(path.to_path_buf())
}

fn long_path(root: &Path) -> PathBuf {
    let mut path = root.to_path_buf();
    let mut parts = 0;
    while path.as_os_str().len() < 300 {
        path.push(format!("segment-{parts:02}-{}", "d".repeat(30)));
        parts += 1;
    }
    path.join("deep.txt")
}

#[cfg(windows)]
#[allow(
    clippy::disallowed_methods,
    reason = "the fixture runs attrib on its own temp file"
)]
fn set_hidden(path: &Path) -> io::Result<()> {
    let status = std::process::Command::new("attrib")
        .arg("+H")
        .arg(path)
        .status()?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| io::Error::other("attrib failed"))
}

#[cfg(not(windows))]
fn set_hidden(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn link_out(root: &Path, target: &Path) -> Option<PathBuf> {
    let link = root.join(if cfg!(windows) {
        "junction-out"
    } else {
        "symlink-out"
    });
    link_dir(&link, target).ok()?;
    Some(link)
}

/// Plants a symlink (Windows: a junction) at `link` pointing to the folder `target`.
#[cfg(windows)]
#[allow(
    clippy::disallowed_methods,
    reason = "the fixture runs mklink to plant a junction in its own temp tree"
)]
pub(crate) fn link_dir(link: &Path, target: &Path) -> io::Result<()> {
    let status = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .status()?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| io::Error::other("mklink failed"))
}

/// Plants a symlink (Windows: a junction) at `link` pointing to the folder `target`.
#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture plants an escaping symlink"
)]
pub(crate) fn link_dir(link: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Moves a folder inside a test tree, to put a link where it was.
#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture moves a folder of its own temp tree"
)]
pub(crate) fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    reason = "guard test fixture plants a relative symlink inside its own tree"
)]
fn inner_link(root: &Path) -> Option<PathBuf> {
    let link = root.join(LINK_NAME);
    std::os::unix::fs::symlink(TARGET_A, &link).ok()?;
    Some(link)
}

#[cfg(not(unix))]
fn inner_link(_root: &Path) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_builds_every_planted_entry() {
        let fx = set().unwrap();
        assert!(fx.marker_file.is_file());
        assert!(fx.rare_file.extension().is_some_and(|e| e == RARE_EXT));
        assert!(fx.hidden_file.is_file());
        assert!(
            fs::metadata(&fx.readonly_file)
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(fx.hard_link.is_file());
        assert!(fx.long_file.as_os_str().len() >= 300);
        assert!(fx.alias_source.is_file());
        assert!(fx.outside.path().join("outside.txt").is_file());
        assert!(fx.root().is_dir());
        assert!(fx.unreadable_dir.is_dir());
    }

    #[test]
    fn link_out_points_outside_when_created() {
        let fx = set().unwrap();
        if let Some(link) = &fx.link_out {
            assert!(link.join("outside.txt").is_file());
        }
    }
}
