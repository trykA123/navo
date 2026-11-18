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

    /// True when some inserted word starts with `byte` (after case folding).
    pub(super) fn starts_with_byte(&self, byte: u8) -> bool {
        self.child(0, self.fold(byte)).is_some()
    }

    /// Returns every inserted word that prefixes `text`, longest first, as (length, value).
    /// Walks the trie once; the node list stays on the stack unless it holds over 4 hits.
    pub(super) fn matches<'a>(&'a self, text: &str) -> Matches<'a, T> {
        let mut hits = Hits::default();
        let mut at = 0;
        for (depth, &byte) in text.as_bytes().iter().enumerate() {
            let Some(next) = self.child(at, self.fold(byte)) else {
                break;
            };
            at = next;
            if self.nodes.get(at).is_some_and(|node| !node.ends.is_empty()) {
                hits.push((depth + 1, at));
            }
        }
        let left = hits.len;
        Matches {
            trie: self,
            hits,
            left,
            values: [].iter(),
            len: 0,
        }
    }
}

/// Inline list of (length, node) hits; spills to the heap only past `INLINE` entries.
#[derive(Default)]
struct Hits {
    inline: [(usize, usize); INLINE],
    len: usize,
    spill: Vec<(usize, usize)>,
}

const INLINE: usize = 4;

impl Hits {
    fn push(&mut self, hit: (usize, usize)) {
        match self.inline.get_mut(self.len) {
            Some(slot) => *slot = hit,
            None => self.spill.push(hit),
        }
        self.len += 1;
    }

    fn get(&self, index: usize) -> Option<(usize, usize)> {
        match index.checked_sub(INLINE) {
            Some(spilled) => self.spill.get(spilled).copied(),
            None => self.inline.get(index).copied(),
        }
    }
}

/// Iterator over the words found by [`Trie::matches`], longest first.
pub(super) struct Matches<'a, T> {
    trie: &'a Trie<T>,
    hits: Hits,
    left: usize,
    values: std::slice::Iter<'a, T>,
    len: usize,
}

impl<T: Copy> Iterator for Matches<'_, T> {
    type Item = (usize, T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(value) = self.values.next() {
                return Some((self.len, *value));
            }
            self.left = self.left.checked_sub(1)?;
            let (len, at) = self.hits.get(self.left)?;
            self.len = len;
            self.values = self.trie.nodes.get(at)?.ends.iter();
        }
    }
}
