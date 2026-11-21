//! Picks the remote ref for a revision: the candidate names to ask for, then the first advertised.

use sidewise_git_address::RemoteRev;

use crate::problem::RemoteProblem;

/// The ref names to request for a revision: `HEAD`, a full `refs/…` name, or head then tag.
pub fn candidates(rev: &RemoteRev) -> Vec<String> {
    let text = rev.as_str();
    if text == "HEAD" || text.starts_with("refs/") {
        vec![text.to_owned()]
    } else {
        vec![format!("refs/heads/{text}"), format!("refs/tags/{text}")]
    }
}

/// Returns the first candidate the server advertised, or `NoSuchRevision`.
pub fn choose(advertised: &[String], rev: &RemoteRev) -> Result<String, RemoteProblem> {
    candidates(rev)
        .into_iter()
        .find(|name| advertised.contains(name))
        .ok_or(RemoteProblem::NoSuchRevision)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(text: &str) -> RemoteRev {
        RemoteRev::parse(text).unwrap()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn candidates_by_form() {
        assert_eq!(candidates(&rev("")), names(&["HEAD"]));
        assert_eq!(candidates(&rev("HEAD")), names(&["HEAD"]));
        assert_eq!(
            candidates(&rev("refs/pull/1/head")),
            names(&["refs/pull/1/head"])
        );
        assert_eq!(
            candidates(&rev("v1")),
            names(&["refs/heads/v1", "refs/tags/v1"])
        );
    }

    #[test]
    fn heads_before_tags() {
        let both = names(&["refs/tags/v1", "refs/heads/v1"]);
        assert_eq!(choose(&both, &rev("v1")).unwrap(), "refs/heads/v1");
        let tag = names(&["refs/tags/v1"]);
        assert_eq!(choose(&tag, &rev("v1")).unwrap(), "refs/tags/v1");
        assert_eq!(choose(&names(&["HEAD"]), &rev("HEAD")).unwrap(), "HEAD");
    }

    #[test]
    fn none_is_no_such_revision() {
        let other = names(&["refs/heads/main"]);
        assert_eq!(
            choose(&other, &rev("v1")),
            Err(RemoteProblem::NoSuchRevision)
        );
        assert_eq!(
            choose(&[], &rev("HEAD")),
            Err(RemoteProblem::NoSuchRevision)
        );
    }
}
