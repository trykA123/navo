#[derive(Debug, Clone)]
pub struct Ring<T: Copy> {
    items: Vec<T>,
    capacity: usize,
    next: usize,
    pushed: u64,
}

impl<T: Copy> Ring<T> {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            items: Vec::with_capacity(capacity),
            capacity,
            next: 0,
            pushed: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.items.len() < self.capacity {
            self.items.push(item);
        } else {
            self.items[self.next] = item;
        }
        self.next = (self.next + 1) % self.capacity;
        self.pushed += 1;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.pushed - self.items.len() as u64
    }

    #[must_use]
    pub fn allocated(&self) -> usize {
        self.items.capacity()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let split = if self.items.len() < self.capacity {
            0
        } else {
            self.next
        };
        self.items[split..].iter().chain(&self.items[..split])
    }
}
