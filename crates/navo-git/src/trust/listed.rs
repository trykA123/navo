use std::path::Path;

use crate::GitError;

use super::{Guard, os_from_bytes};

impl Guard {
    /// Reads the git dir Git printed (`rev-parse --absolute-git-dir`); blocked when it is not a path.
    pub(crate) fn read_reported(stdout: &[u8]) -> Result<Self, GitError> {
        let line = stdout.strip_suffix(b"\n").unwrap_or(stdout);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        match os_from_bytes(line) {
            Some(dir) if !dir.is_empty() && !line.contains(&b'\n') => Self::read(Path::new(&dir)),
            _ => Ok(Self::unreadable()),
        }
    }

    /// True when a FIFO, device or socket stands in for a config file, so Git itself would hang.
    pub(crate) fn has_special_file(&self) -> bool {
        self.special_file
    }

    /// Adds what Git read for the `local` and `worktree` scopes (`config --list -z --show-scope
    /// --includes`); blocks when Git found a key or filter this reader missed, or on bad output.
    pub(crate) fn cross_check(mut self, listed: &[u8]) -> Self {
        let Some(found) = parse(listed) else {
            return Self::unreadable();
        };
        let missed = found.blocked
            || found.keys.keys().any(|key| !self.keys.contains_key(key))
            || !found.filters.is_subset(&self.filters);
        for (key, values) in found.keys {
            self.keys.entry(key).or_default().0.extend(values.0);
        }
        self.filters.extend(found.filters);
        self.blocked |= missed;
        self
    }
}

fn parse(listed: &[u8]) -> Option<Guard> {
    let mut found = Guard::default();
    let mut tokens = listed.split(|byte| *byte == 0);
    while let Some(scope) = tokens.next() {
        if scope.is_empty() {
            return tokens.next().is_none().then_some(found);
        }
        let entry = tokens.next()?;
        if matches!(scope, b"local" | b"worktree") {
            let mut parts = entry.splitn(2, |byte| *byte == b'\n');
            let key = parts.next()?;
            note_flat(&mut found, key, parts.next())?;
        }
    }
    None
}

/// Splits `section[.subsection].key` the way Git flattens it: the subsection may hold dots.
fn note_flat(found: &mut Guard, flat: &[u8], value: Option<&[u8]>) -> Option<()> {
    let first = flat.iter().position(|byte| *byte == b'.')?;
    let last = flat.iter().rposition(|byte| *byte == b'.')?;
    let section = &flat[..first];
    let subsection = (first < last).then(|| &flat[first + 1..last]);
    let key = std::str::from_utf8(&flat[last + 1..]).ok()?;
    if let Some(values) = found.note(section, subsection, key) {
        values.extend(value.map(<[u8]>::to_vec));
    }
    if section == b"filter"
        && let Some(sub) = subsection
    {
        found.add_filter(sub);
    }
    Some(())
}
