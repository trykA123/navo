#[derive(Debug, Clone)]
struct Node<T> {
    next: Vec<(u8, usize)>,
    ends: Vec<T>,
}

impl<T> Default for Node<T> {
    fn default() -> Self {
        Self {
            next: Vec::new(),
            ends: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Trie<T> {
    nodes: Vec<Node<T>>,
    case_ignored: bool,
}

impl<T: Copy> Trie<T> {
    pub(super) fn new(case_ignored: bool) -> Self {
        Self {
            nodes: vec![Node::default()],
            case_ignored,
        }
    }

    fn fold(&self, byte: u8) -> u8 {
        if self.case_ignored {
            byte.to_ascii_lowercase()
        } else {
            byte
        }
    }

    pub(super) fn insert(&mut self, word: &str, value: T) {
        let mut at = 0;
        for &byte in word.as_bytes() {
            at = self.child_or_insert(at, self.fold(byte));
        }
        if let Some(node) = self.nodes.get_mut(at) {
            node.ends.push(value);
        }
    }

    fn child(&self, at: usize, byte: u8) -> Option<usize> {
        let next = &self.nodes.get(at)?.next;
        let found = next.binary_search_by_key(&byte, |(b, _)| *b).ok()?;
        next.get(found).map(|(_, child)| *child)
    }

    fn child_or_insert(&mut self, at: usize, byte: u8) -> usize {
        if let Some(found) = self.child(at, byte) {
            return found;
        }
        let index = self.nodes.len();
        self.nodes.push(Node::default());
        if let Some(node) = self.nodes.get_mut(at) {
            let slot = node.next.partition_point(|(b, _)| *b < byte);
            node.next.insert(slot, (byte, index));
        }
        index
    }

    pub(super) fn matches(&self, text: &str) -> impl Iterator<Item = (usize, T)> {
        let mut ends = Vec::new();
        let mut at = 0;
        for (depth, &byte) in text.as_bytes().iter().enumerate() {
            let Some(next) = self.child(at, self.fold(byte)) else {
                break;
            };
            at = next;
            ends.push((depth + 1, at));
        }
        ends.into_iter().rev().flat_map(move |(len, at)| {
            self.nodes
                .get(at)
                .into_iter()
                .flat_map(|node| node.ends.iter())
                .map(move |value| (len, *value))
        })
    }
}
