use super::NO_PARENT;

const NO_LINK: u32 = u32::MAX;

#[derive(Debug, Clone, Default)]
pub(super) struct Children {
    first: Vec<u32>,
    next: Vec<u32>,
    previous: Vec<u32>,
    waiting: Vec<u32>,
}

impl Children {
    pub(super) fn with_capacity(entries: usize) -> Self {
        Self {
            first: Vec::with_capacity(entries),
            next: Vec::with_capacity(entries),
            previous: Vec::with_capacity(entries),
            waiting: Vec::new(),
        }
    }

    pub(super) fn rebuild(parents: &[u32], is_live: impl Fn(u32) -> bool) -> Self {
        let mut children = Self::with_capacity(parents.len());
        children.first.resize(parents.len(), NO_LINK);
        children.next.resize(parents.len(), NO_LINK);
        children.previous.resize(parents.len(), NO_LINK);
        for (id, &parent) in (0u32..).zip(parents) {
            if is_live(id) {
                children.attach(id, parent);
            }
        }
        children
    }

    pub(super) fn memory_bytes(&self) -> usize {
        (self.first.capacity()
            + self.next.capacity()
            + self.previous.capacity()
            + self.waiting.capacity())
            * size_of::<u32>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.first.shrink_to_fit();
        self.next.shrink_to_fit();
        self.previous.shrink_to_fit();
        self.waiting.shrink_to_fit();
    }

    pub(super) fn add(&mut self, id: u32, parents: &[u32]) {
        self.first.push(NO_LINK);
        self.next.push(NO_LINK);
        self.previous.push(NO_LINK);
        self.attach(id, parents[id as usize]);
        if self
            .waiting
            .iter()
            .any(|&child| parents[child as usize] == id)
        {
            let ready: Vec<u32> = self
                .waiting
                .extract_if(.., |&mut child| parents[child as usize] == id)
                .collect();
            for child in ready {
                self.attach(child, id);
            }
        }
    }

    pub(super) fn reparent(&mut self, child: u32, from: u32, to: u32) {
        self.detach(child, from);
        self.attach(child, to);
    }

    pub(super) fn detach(&mut self, child: u32, parent: u32) {
        if parent as usize >= self.first.len() {
            self.waiting.retain(|&waiting| waiting != child);
            return;
        }
        let at = child as usize;
        let (previous, next) = (self.previous[at], self.next[at]);
        if previous != NO_LINK {
            self.next[previous as usize] = next;
        } else if self.first[parent as usize] == child {
            self.first[parent as usize] = next;
        }
        if next != NO_LINK {
            self.previous[next as usize] = previous;
        }
        self.next[at] = NO_LINK;
        self.previous[at] = NO_LINK;
    }

    pub(super) fn of(&self, parent: u32) -> impl Iterator<Item = u32> + '_ {
        let first = self.first.get(parent as usize).copied().unwrap_or(NO_LINK);
        let linked = |child: u32| (child != NO_LINK).then_some(child);
        std::iter::successors(linked(first), move |&child| {
            linked(self.next[child as usize])
        })
    }

    fn attach(&mut self, child: u32, parent: u32) {
        if parent as usize >= self.first.len() {
            if parent != NO_PARENT {
                self.waiting.push(child);
            }
            return;
        }
        let head = self.first[parent as usize];
        self.next[child as usize] = head;
        self.previous[child as usize] = NO_LINK;
        if head != NO_LINK {
            self.previous[head as usize] = child;
        }
        self.first[parent as usize] = child;
    }
}
