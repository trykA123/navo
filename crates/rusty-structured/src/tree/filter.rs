use super::{Key, NodeId, Row, Tree};

#[derive(Debug, Clone, Default)]
pub struct Filtered {
    pub(super) filter: String,
    pub(super) rows: Vec<Row>,
    pub(super) cursor: usize,
    pub(super) matching: usize,
}

impl Filtered {
    pub(super) fn new(tree: &Tree, filter: &str) -> Self {
        let rows = filtered_rows(tree, filter);
        let is_match = |row: &&Row| matches(tree.key(row.node), filter);
        Self {
            filter: filter.to_owned(),
            cursor: rows.iter().position(|row| is_match(&row)).unwrap_or(0),
            matching: rows.iter().filter(is_match).count(),
            rows,
        }
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }
}

pub(super) fn matches(key: Key<'_>, needle: &str) -> bool {
    let Key::Name(name) = key else {
        return false;
    };
    if needle.is_ascii() {
        let (hay, needle) = (name.as_bytes(), needle.as_bytes());
        return needle.is_empty()
            || hay
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle));
    }
    name.to_lowercase().contains(&needle.to_lowercase())
}

fn kept_nodes(tree: &Tree, filter: &str) -> Vec<bool> {
    let mut keep = vec![false; tree.len()];
    for id in tree.ids() {
        if !matches(tree.key(id), filter) {
            continue;
        }
        let mut at = Some(id);
        while let Some(node) = at {
            let Some(slot) = keep.get_mut(node as usize) else {
                break;
            };
            if *slot {
                break;
            }
            *slot = true;
            at = tree.parent(node);
        }
    }
    keep
}

fn filtered_rows(tree: &Tree, filter: &str) -> Vec<Row> {
    let keep = kept_nodes(tree, filter);
    let Some(root) = tree.root() else {
        return Vec::new();
    };
    let is_kept = |id: NodeId| keep.get(id as usize).copied().unwrap_or(false);
    let mut rows = Vec::new();
    let mut stack = vec![(root, 0u16)];
    while let Some((at, depth)) = stack.pop() {
        if !is_kept(at) {
            continue;
        }
        rows.push(Row { node: at, depth });
        let children: Vec<NodeId> = tree.children(at).filter(|id| is_kept(*id)).collect();
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, depth.saturating_add(1))),
        );
    }
    rows
}
