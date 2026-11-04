use std::collections::HashMap;

use navo_core::Step;

use super::super::{Parents, Side, check_name, segments, target_steps};
use super::parts::Written;
use crate::model::{EntryKind, Row};
use crate::session::Refusal;

/// The target side's own spelling of each folder pair whose two names differ.
pub(super) struct Spellings {
    folders: HashMap<(Written, String), Vec<String>>,
}

impl Spellings {
    pub(super) fn of(rows: &[Row]) -> Self {
        let mut folders = HashMap::new();
        for row in rows {
            let (Some(left), Some(right)) = (row.left.as_deref(), row.right.as_deref()) else {
                continue;
            };
            if left.kind != EntryKind::Folder || right.kind != EntryKind::Folder {
                continue;
            }
            let (left_text, right_text) =
                (left.rel.as_path().to_text(), right.rel.as_path().to_text());
            if left_text == right_text {
                continue;
            }
            if let (Ok(left_names), Ok(right_names)) = (segments(&left.rel), segments(&right.rel)) {
                folders.insert((Written::Right, left_text), right_names);
                folders.insert((Written::Left, right_text), left_names);
            }
        }
        Self { folders }
    }

    /// `names` as the written side spells them: the deepest paired parent takes its target name.
    /// `None` when no parent is spelled differently.
    pub(super) fn respell(&self, written: Written, names: &[String]) -> Option<Vec<String>> {
        if self.folders.is_empty() {
            return None;
        }
        (1..names.len()).rev().find_map(|depth| {
            let (parent, rest) = names.split_at_checked(depth)?;
            let target = self.folders.get(&(written, parent.join("/")))?;
            Some(target.iter().chain(rest).cloned().collect())
        })
    }
}

/// A new file's steps rebuilt under the target side's spelling of its parent folders.
pub(super) fn respelled(
    mut steps: Vec<Step>,
    names: &[String],
    target: &Side,
) -> Result<Vec<Step>, Refusal> {
    let Some(Step::CopyFile { source, size, .. }) = steps.pop() else {
        return Err(Refusal::NotAFile);
    };
    for name in names {
        check_name(name, target.names)?;
    }
    let (destination, mut steps) = target_steps(&target.root, names, Parents::Make)?;
    steps.push(Step::CopyFile {
        source,
        target: destination,
        size,
    });
    Ok(steps)
}
