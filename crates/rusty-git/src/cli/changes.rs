use std::collections::HashMap;

use crate::{
    Change, ChangeStatus, GitError, GitPath, IndexEntry, ObjectId, Stage, StageSet, Target,
    TreeKind,
};

use super::CliGit;
use super::parse::{parse_oid, protocol, tree_kind};

pub(super) fn read_changes(
    cli: &CliGit,
    a: &str,
    target: Target<'_>,
) -> Result<Vec<Change>, GitError> {
    let output = match target {
        Target::Rev(b) => cli.run_ok(&[
            "diff-tree",
            "-r",
            "-z",
            "--raw",
            "--no-abbrev",
            "--no-renames",
            "--end-of-options",
            a,
            b,
            "--",
        ])?,
        Target::Index => cli.run_ok(&[
            "diff",
            "--raw",
            "-z",
            "--no-abbrev",
            "--no-renames",
            "--cached",
            "--end-of-options",
            a,
            "--",
        ])?,
        Target::Worktree => cli.run_ok(&[
            "diff",
            "--raw",
            "-z",
            "--no-abbrev",
            "--no-renames",
            "--end-of-options",
            a,
            "--",
        ])?,
    };
    let mut changes = parse_raw(cli, &output.stdout)?;
    if matches!(target, Target::Worktree) {
        apply_worktree_conflicts(cli, &mut changes)?;
    }
    Ok(changes)
}

fn parse_raw(cli: &CliGit, output: &[u8]) -> Result<Vec<Change>, GitError> {
    let mut fields = output.split(|byte| *byte == 0);
    let mut changes = Vec::new();
    let mut positions: HashMap<GitPath, usize> = HashMap::new();
    let mut unmerged = None;
    while let Some(header) = fields.next() {
        if header.is_empty() {
            continue;
        }
        let path = fields.next().ok_or_else(protocol)?;
        if path.is_empty() {
            return Err(protocol());
        }
        let (old_id, new_id, mut status) = parse_raw_header(header)?;
        let path = GitPath::new(path.to_vec());
        if matches!(status, ChangeStatus::Conflict { .. }) {
            status = ChangeStatus::Conflict {
                stages: conflict_stages(cli, &mut unmerged, &path)?,
            };
        }
        add_change(
            &mut changes,
            &mut positions,
            Change {
                path,
                status,
                old_id,
                new_id,
            },
        );
    }
    Ok(changes)
}

fn apply_worktree_conflicts(cli: &CliGit, changes: &mut Vec<Change>) -> Result<(), GitError> {
    let unmerged = read_unmerged(cli)?;
    if unmerged.is_empty() {
        return Ok(());
    }
    let positions: HashMap<GitPath, usize> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| (change.path.clone(), index))
        .collect();
    let mut missing = Vec::new();
    for (path, stages) in unmerged {
        if let Some(&index) = positions.get(&path) {
            let change = &mut changes[index];
            change.status = ChangeStatus::Conflict { stages };
            change.old_id = None;
            change.new_id = None;
        } else {
            missing.push(Change {
                path,
                status: ChangeStatus::Conflict { stages },
                old_id: None,
                new_id: None,
            });
        }
    }
    missing.sort_by(|left, right| left.path.cmp(&right.path));
    changes.extend(missing);
    Ok(())
}

fn parse_raw_header(
    header: &[u8],
) -> Result<(Option<ObjectId>, Option<ObjectId>, ChangeStatus), GitError> {
    let header = header.strip_prefix(b":").ok_or_else(protocol)?;
    let header = std::str::from_utf8(header).map_err(|_| protocol())?;
    let mut fields = header.split_ascii_whitespace();
    let _old_mode = fields.next().ok_or_else(protocol)?;
    let _new_mode = fields.next().ok_or_else(protocol)?;
    let old_id = optional_oid(fields.next().ok_or_else(protocol)?)?;
    let new_id = optional_oid(fields.next().ok_or_else(protocol)?)?;
    let status = fields.next().ok_or_else(protocol)?;
    if fields.next().is_some() {
        return Err(protocol());
    }
    let status = raw_status(status)?;
    if matches!(status, ChangeStatus::Conflict { .. }) {
        return Ok((None, None, status));
    }
    Ok((old_id, new_id, status))
}

fn raw_status(status: &str) -> Result<ChangeStatus, GitError> {
    match status.as_bytes() {
        b"A" => Ok(ChangeStatus::Added),
        b"D" => Ok(ChangeStatus::Deleted),
        b"M" => Ok(ChangeStatus::Modified),
        b"T" => Ok(ChangeStatus::TypeChanged),
        b"U" => Ok(ChangeStatus::Conflict {
            stages: StageSet::empty(),
        }),
        _ => Err(protocol()),
    }
}

fn optional_oid(text: &str) -> Result<Option<ObjectId>, GitError> {
    let id = parse_oid(text.as_bytes())?;
    if id.as_bytes().iter().all(|byte| *byte == 0) {
        Ok(None)
    } else {
        Ok(Some(id))
    }
}

fn add_change(changes: &mut Vec<Change>, positions: &mut HashMap<GitPath, usize>, change: Change) {
    if let Some(&index) = positions.get(&change.path) {
        let existing = &mut changes[index];
        if !matches!(existing.status, ChangeStatus::Conflict { .. })
            && matches!(change.status, ChangeStatus::Conflict { .. })
        {
            *existing = change;
        }
        return;
    }
    positions.insert(change.path.clone(), changes.len());
    changes.push(change);
}

fn conflict_stages(
    cli: &CliGit,
    unmerged: &mut Option<HashMap<GitPath, StageSet>>,
    path: &GitPath,
) -> Result<StageSet, GitError> {
    if unmerged.is_none() {
        *unmerged = Some(read_unmerged(cli)?);
    }
    Ok(unmerged
        .as_ref()
        .and_then(|stages| stages.get(path))
        .copied()
        .unwrap_or_default())
}

fn read_unmerged(cli: &CliGit) -> Result<HashMap<GitPath, StageSet>, GitError> {
    let output = cli.run_ok(&["ls-files", "-u", "-z", "--full-name", "--"])?;
    let mut stages: HashMap<GitPath, StageSet> = HashMap::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let entry = parse_index_record(record)?;
        let Some(stage) = stage_of(entry.stage) else {
            continue;
        };
        stages
            .entry(entry.path)
            .and_modify(|set| *set = set.with(stage))
            .or_insert_with(|| StageSet::empty().with(stage));
    }
    Ok(stages)
}

pub(super) fn read_index_entries(cli: &CliGit, stage: Stage) -> Result<Vec<IndexEntry>, GitError> {
    let output = cli.run_ok(&["ls-files", "-s", "-z", "--full-name", "--"])?;
    let wanted = stage_number(stage);
    let mut entries = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let entry = parse_index_record(record)?;
        if entry.stage != wanted && entry.stage != 0 {
            continue;
        }
        entries.push(IndexEntry {
            path: entry.path,
            id: entry.id,
            kind: entry.kind,
        });
    }
    Ok(entries)
}

struct IndexRecord {
    id: ObjectId,
    stage: u8,
    kind: TreeKind,
    path: GitPath,
}

fn parse_index_record(record: &[u8]) -> Result<IndexRecord, GitError> {
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(protocol)?;
    let header = std::str::from_utf8(&record[..tab]).map_err(|_| protocol())?;
    let mut fields = header.split_ascii_whitespace();
    let mode = fields.next().ok_or_else(protocol)?;
    let id = parse_oid(fields.next().ok_or_else(protocol)?.as_bytes())?;
    let stage = fields.next().ok_or_else(protocol)?;
    if fields.next().is_some() {
        return Err(protocol());
    }
    Ok(IndexRecord {
        id,
        stage: stage.parse::<u8>().map_err(|_| protocol())?,
        kind: tree_kind(mode.as_bytes()),
        path: GitPath::new(record[tab + 1..].to_vec()),
    })
}

fn stage_number(stage: Stage) -> u8 {
    match stage {
        Stage::Base => 1,
        Stage::Ours => 2,
        Stage::Theirs => 3,
    }
}

fn stage_of(number: u8) -> Option<Stage> {
    match number {
        1 => Some(Stage::Base),
        2 => Some(Stage::Ours),
        3 => Some(Stage::Theirs),
        _ => None,
    }
}
