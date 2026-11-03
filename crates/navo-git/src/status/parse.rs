use super::dirs::Severity;
use super::{DirMap, Head, RepoStatus, Upstream};
use crate::cli::parse::parse_oid;
use crate::{GitError, ObjectId};

#[derive(Default)]
struct Headers {
    oid: Option<String>,
    head: Option<String>,
    upstream: Option<String>,
    ab: Option<(u32, u32)>,
}

/// Reads `git status --porcelain=v2 --branch -z` output.
///
/// With `capped`, a last record cut by the size cap is counted when its type is
/// readable and ignored otherwise; without it, such a record is an error.
pub fn parse_porcelain_v2(output: &[u8], capped: bool) -> Result<RepoStatus, GitError> {
    let mut segments: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let tail = segments.pop().unwrap_or_default();
    if !tail.is_empty() {
        if !capped {
            return Err(protocol("record is not terminated"));
        }
        segments.push(tail);
    }
    let cut = (capped && !tail.is_empty()).then_some(segments.len());
    let mut status = empty_status(capped);
    let mut headers = Headers::default();
    let mut skip_original = false;
    for (index, segment) in segments.iter().enumerate() {
        if std::mem::take(&mut skip_original) {
            continue;
        }
        let last_cut = cut == Some(index + 1);
        match (record_kind(segment), last_cut) {
            (Some(b'#'), false) => read_header(&mut headers, segment)?,
            (Some(b'#'), true) | (Some(b'!'), _) => {}
            (Some(kind @ (b'1' | b'2' | b'u' | b'?')), _) => {
                count(&mut status, kind, segment);
                skip_original = kind == b'2';
            }
            (_, true) => {}
            _ => return Err(protocol("unknown record")),
        }
    }
    finish(status, headers)
}

fn empty_status(capped: bool) -> RepoStatus {
    RepoStatus {
        head: Head::Unborn(String::new()),
        upstream: None,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        operation: None,
        dirs: DirMap::default(),
        capped,
        fetched_ms: None,
    }
}

fn count(status: &mut RepoStatus, kind: u8, segment: &[u8]) {
    let (counter, severity, fields) = match kind {
        b'1' => (&mut status.changed, Severity::Modified, 9),
        b'2' => (&mut status.changed, Severity::Modified, 10),
        b'u' => (&mut status.conflicted, Severity::Conflict, 11),
        _ => (&mut status.untracked, Severity::Modified, 2),
    };
    *counter = counter.saturating_add(1);
    if let Some(path) = segment.splitn(fields, |byte| *byte == b' ').nth(fields - 1) {
        status.dirs.note(path, severity);
    }
}

fn record_kind(segment: &[u8]) -> Option<u8> {
    match segment {
        [kind, b' ', ..] => Some(*kind),
        _ => None,
    }
}

fn read_header(headers: &mut Headers, segment: &[u8]) -> Result<(), GitError> {
    let text = std::str::from_utf8(segment).map_err(|_| protocol("header is not UTF-8"))?;
    let Some((key, value)) = text.trim_start_matches("# ").split_once(' ') else {
        return Ok(());
    };
    match key {
        "branch.oid" => headers.oid = Some(value.to_owned()),
        "branch.head" => headers.head = Some(value.to_owned()),
        "branch.upstream" => headers.upstream = Some(value.to_owned()),
        "branch.ab" => headers.ab = Some(parse_ab(value)?),
        _ => {}
    }
    Ok(())
}

fn parse_ab(value: &str) -> Result<(u32, u32), GitError> {
    let (ahead, behind) = value.split_once(' ').ok_or_else(|| protocol("branch.ab"))?;
    let ahead = ahead.strip_prefix('+').and_then(|text| text.parse().ok());
    let behind = behind.strip_prefix('-').and_then(|text| text.parse().ok());
    ahead
        .zip(behind)
        .ok_or_else(|| protocol("branch.ab counts"))
}

fn finish(mut status: RepoStatus, headers: Headers) -> Result<RepoStatus, GitError> {
    let name = headers
        .head
        .ok_or_else(|| protocol("branch.head missing"))?;
    let oid = headers.oid.ok_or_else(|| protocol("branch.oid missing"))?;
    status.head = match (name.as_str(), oid.as_str()) {
        ("(detached)", id) => Head::Detached(commit(id)?),
        (_, "(initial)") => Head::Unborn(name),
        _ => Head::Branch(name),
    };
    status.upstream = headers.upstream.map(|name| {
        let (ahead, behind) = headers.ab.unwrap_or_default();
        Upstream {
            name,
            ahead,
            behind,
        }
    });
    Ok(status)
}

fn commit(id: &str) -> Result<ObjectId, GitError> {
    parse_oid(id.as_bytes()).map_err(|_| protocol("branch.oid"))
}

fn protocol(detail: &str) -> GitError {
    GitError::Protocol {
        detail: detail.to_owned(),
    }
}
