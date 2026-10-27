use std::sync::{Arc, OnceLock};

use super::{Classifier, IoQueue, Limits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    Listing,
    Preview,
    Size,
    Hash,
    Module,
}

impl JobKind {
    pub const ALL: [Self; 5] = [
        Self::Listing,
        Self::Preview,
        Self::Size,
        Self::Hash,
        Self::Module,
    ];

    #[must_use]
    pub fn limits(self) -> Limits {
        let (ssd, hdd, network, removable) = match self {
            Self::Listing => return Limits::LISTING,
            Self::Preview => (2, 1, 2, 1),
            Self::Size | Self::Hash => (2, 1, 1, 1),
            Self::Module => (1, 1, 1, 1),
        };
        Limits {
            ssd,
            hdd,
            network,
            removable,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

pub struct Pools {
    queues: [Arc<IoQueue>; 5],
}

impl Pools {
    #[must_use]
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<Pools> = OnceLock::new();
        SHARED.get_or_init(Self::new)
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            queues: JobKind::ALL.map(|kind| Arc::new(IoQueue::with_limits(kind.limits()))),
        }
    }

    #[must_use]
    pub fn with(classifier: &Classifier) -> Self {
        Self {
            queues: JobKind::ALL
                .map(|kind| Arc::new(IoQueue::with(kind.limits(), Arc::clone(classifier)))),
        }
    }

    #[must_use]
    pub fn queue(&self, kind: JobKind) -> &Arc<IoQueue> {
        &self.queues[kind.index()]
    }
}

impl Default for Pools {
    fn default() -> Self {
        Self::new()
    }
}
