use crate::GitError;

use super::CliGit;
use super::parse::protocol;

const HEADS: &str = "refs/heads/";

const CREDENTIAL_SIGNS: [&[u8]; 8] = [
    b"Authentication failed",
    b"unable to get password",
    b"could not read Username",
    b"could not read Password",
    b"terminal prompts disabled",
    b"Permission denied (publickey",
    b"The requested URL returned error: 401",
    b"The requested URL returned error: 403",
];

/// The checked-out branch and the upstream branch Pull and Push use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracking {
    pub branch: String,
    pub remote: String,
    pub remote_branch: String,
    pub tracking_ref: String,
}

impl CliGit {
    /// Refuses a detached HEAD, no upstream, a local (`.`) upstream, an upstream outside
    /// `refs/heads/` and a remote that starts with `-`.
    pub fn tracking(&self) -> Result<Tracking, GitError> {
        let branch_ref = self.current_branch()?;
        let args = [
            "for-each-ref",
            "--format=%(upstream:remotename)%00%(upstream)%00%(upstream:remoteref)",
            "--end-of-options",
            &branch_ref,
        ];
        let listed = self.run_ok(&args)?;
        let text = std::str::from_utf8(&listed.stdout).map_err(|_| protocol())?;
        let fields: Vec<&str> = text.trim_end_matches(['\n', '\r']).split('\0').collect();
        let [remote, upstream, remote_ref] = fields[..] else {
            return Err(GitError::NoUpstream);
        };
        let remote_branch = remote_ref
            .strip_prefix(HEADS)
            .filter(|name| !name.is_empty());
        let Some(remote_branch) = remote_branch else {
            return Err(GitError::NoUpstream);
        };
        if remote.is_empty() || remote == "." || upstream.is_empty() {
            return Err(GitError::NoUpstream);
        }
        if remote.starts_with('-') {
            return Err(GitError::UnsafeRemote {
                name: remote.to_owned(),
            });
        }
        Ok(Tracking {
            branch: branch_ref
                .strip_prefix(HEADS)
                .unwrap_or(&branch_ref)
                .to_owned(),
            remote: remote.to_owned(),
            remote_branch: remote_branch.to_owned(),
            tracking_ref: upstream.to_owned(),
        })
    }

    /// Commits on the upstream that HEAD lacks, as of the last fetch.
    pub(super) fn behind(&self, tracking: &Tracking) -> Result<u32, GitError> {
        let upstream = self.commit_of(&tracking.tracking_ref)?;
        let head = self.commit_of("HEAD")?;
        self.count_between(head.as_deref(), upstream.as_deref())
    }

    /// Commits on HEAD the remote lacks, as of the last fetch.
    pub(super) fn unpushed(&self, tracking: &Tracking) -> Result<u32, GitError> {
        if self.commit_of("HEAD")?.is_none() {
            return Ok(0);
        }
        if self.commit_of(&tracking.tracking_ref)?.is_some() {
            return self.count(&["HEAD", &format!("^{}", tracking.tracking_ref)], true);
        }
        let remotes = format!("--remotes={}", tracking.remote);
        self.count(&["HEAD", "--not", &remotes], false)
    }

    pub(super) fn count_between(
        &self,
        before: Option<&str>,
        after: Option<&str>,
    ) -> Result<u32, GitError> {
        match (before, after) {
            (_, None) => Ok(0),
            (Some(before), Some(after)) if before == after => Ok(0),
            (Some(before), Some(after)) => self.count(&[after, &format!("^{before}")], true),
            (None, Some(after)) => self.count(&[after], true),
        }
    }

    /// The commit `rev` names, or `None` when it does not exist.
    pub(super) fn commit_of(&self, rev: &str) -> Result<Option<String>, GitError> {
        let revision = format!("{rev}^{{commit}}");
        let args = ["rev-parse", "--verify", "-q", "--end-of-options", &revision];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => {
                let text = std::str::from_utf8(&output.stdout).map_err(|_| protocol())?;
                Ok(Some(text.trim_end().to_owned()))
            }
            Some(1) => Ok(None),
            _ => Err(self.command_error(&args, &output)),
        }
    }

    fn count(&self, revs: &[&str], end_options: bool) -> Result<u32, GitError> {
        let mut args = vec!["rev-list", "--count"];
        if end_options {
            args.push("--end-of-options");
        }
        args.extend_from_slice(revs);
        let output = self.run_ok(&args)?;
        let text = std::str::from_utf8(&output.stdout).map_err(|_| protocol())?;
        text.trim().parse().map_err(|_| protocol())
    }
}

/// The object id of a `FETCH_HEAD` that holds exactly one line, for `refs/heads/<remote_branch>`
/// and marked for merge; `None` for anything else, such as a file another fetch rewrote.
pub(super) fn fetched_oid<'a>(fetch_head: &'a str, remote_branch: &str) -> Option<&'a str> {
    let line = fetch_head.strip_suffix('\n')?;
    let mut fields = line.split('\t');
    let (oid, mark, about) = (fields.next()?, fields.next()?, fields.next()?);
    let named = about.strip_prefix(&format!("branch '{remote_branch}' of "))?;
    let is_hex = matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit());
    let is_single = fields.next().is_none() && !line.contains('\n') && !named.is_empty();
    (is_hex && is_single && mark.is_empty()).then_some(oid)
}

/// Git's English words for a refused login; `LC_ALL=C` keeps them English.
pub(super) fn needs_credentials(stderr: &[u8]) -> bool {
    CREDENTIAL_SIGNS.iter().any(|sign| contains(stderr, sign))
}

/// Git's English words when a fast-forward-only pull meets diverged branches.
pub(super) fn refuses_fast_forward(stderr: &[u8]) -> bool {
    contains(stderr, b"Not possible to fast-forward") || contains(stderr, b"Diverging branches")
}

/// Git's English words when local changes or untracked files block a checkout.
pub(super) fn would_overwrite(stderr: &[u8]) -> bool {
    contains(stderr, b"would be overwritten")
}

/// The `git push --porcelain` flag of the line that updates `refs/heads/<remote_branch>`.
pub(super) fn pushed_flag(stdout: &[u8], remote_branch: &str) -> Option<char> {
    let target = format!("{HEADS}{remote_branch}");
    String::from_utf8_lossy(stdout).lines().find_map(|line| {
        let mut fields = line.split('\t');
        let flag = fields.next()?.chars().next()?;
        let (_, to) = fields.next()?.split_once(':')?;
        (to == target).then_some(flag)
    })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::fetched_oid;

    const OID: &str = "2dd5213cbc6f560175b0a3a034d7c499d445d704";

    #[test]
    fn a_single_fetched_branch_names_its_commit() {
        let text = format!("{OID}\t\tbranch 'main' of /tmp/o\n");
        assert_eq!(fetched_oid(&text, "main"), Some(OID));
    }

    #[test]
    fn any_other_fetch_head_names_nothing() {
        let other = "1".repeat(40);
        let rejected = [
            String::new(),
            format!("{OID}\t\tbranch 'main' of /tmp/o"),
            format!("{OID}\t\tbranch 'dev' of /tmp/o\n"),
            format!("{OID}\tnot-for-merge\tbranch 'main' of /tmp/o\n"),
            format!("{OID}\t\tbranch 'main' of /tmp/o\n{other}\t\tbranch 'main' of /tmp/p\n"),
            format!("{OID}\t\ttag 'main' of /tmp/o\n"),
            "xyz\t\tbranch 'main' of /tmp/o\n".to_owned(),
        ];
        for text in rejected {
            assert_eq!(fetched_oid(&text, "main"), None, "{text:?}");
        }
    }
}
