//! Deferred eviction releases for the large diff (V35): each snapshot the view gets has a
//! number, and its evictions are released only after the view acknowledges that number.

/// Releases one held eviction charge.
pub(crate) type Release = Box<dyn FnOnce()>;

/// What one acknowledgement frees: the engine's paged sequence and held focus charges.
pub(crate) struct Freed {
    pub(crate) seq: Option<u64>,
    pub(crate) releases: Vec<Release>,
}

/// The snapshot counter and the evictions each sent snapshot still holds.
#[derive(Default)]
pub(crate) struct Acks {
    /// The number of the last snapshot sent; the first is 1.
    sent: u64,
    /// The highest paged sequence noted since the last snapshot.
    pending_seq: Option<u64>,
    /// Focus evictions applied to the state but not sent yet.
    pending_holds: Vec<Release>,
    /// Paged sequences by the snapshot that first carried them.
    seqs: Vec<(u64, u64)>,
    /// Focus eviction charges by the snapshot that first carried them.
    held: Vec<(u64, Release)>,
}

impl Acks {
    /// Note a paged event's sequence; the next snapshot carries it.
    pub(crate) fn note_paged(&mut self, seq: u64) {
        self.pending_seq = Some(self.pending_seq.map_or(seq, |old| old.max(seq)));
    }

    /// Hold a focus eviction's charge until the next snapshot is acknowledged.
    pub(crate) fn hold(&mut self, release: Release) {
        self.pending_holds.push(release);
    }

    /// Number the snapshot about to be sent and tie the pending evictions to it.
    pub(crate) fn next_snapshot(&mut self) -> u64 {
        self.sent = self.sent.saturating_add(1);
        if let Some(seq) = self.pending_seq.take() {
            self.seqs.push((self.sent, seq));
        }
        let sent = self.sent;
        self.held
            .extend(self.pending_holds.drain(..).map(|release| (sent, release)));
        sent
    }

    /// Free everything the view applied with snapshots up to `upto`; each snapshot holds the
    /// whole state, so applying one applies every earlier eviction too.
    pub(crate) fn acknowledge(&mut self, upto: u64) -> Freed {
        let seq = self
            .seqs
            .iter()
            .filter(|(snapshot, _)| *snapshot <= upto)
            .map(|(_, seq)| *seq)
            .max();
        self.seqs.retain(|(snapshot, _)| *snapshot > upto);
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(snapshot, _)| *snapshot <= upto);
        self.held = later;
        Freed {
            seq,
            releases: now.into_iter().map(|(_, release)| release).collect(),
        }
    }
}
