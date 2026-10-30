use rusty_git::TreeEntry;

pub(super) fn find<'a>(entries: &'a [TreeEntry], path: &[u8]) -> Option<&'a TreeEntry> {
    entries.iter().find(|entry| entry.path.as_bytes() == path)
}

pub(super) fn has_descendants(entries: &[TreeEntry], path: &[u8]) -> bool {
    if path.is_empty() {
        return !entries.is_empty();
    }
    let mut prefix = path.to_vec();
    prefix.push(b'/');
    entries
        .iter()
        .any(|entry| entry.path.as_bytes().starts_with(&prefix))
}
