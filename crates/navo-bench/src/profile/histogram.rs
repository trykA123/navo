use crate::privacy::{Report, SafeText};

/// Inclusive upper bounds plus a final open bucket.
pub(super) struct Layout {
    pub(super) group: SafeText,
    pub(super) bounds: &'static [u64],
    pub(super) labels: &'static [SafeText],
}

#[derive(Clone, Debug, Default)]
pub struct Histogram {
    counts: Vec<u64>,
    bytes: Vec<u64>,
}

impl Histogram {
    pub(super) fn new(layout: &Layout) -> Self {
        let slots = layout.bounds.len() + 1;
        Self {
            counts: vec![0; slots],
            bytes: vec![0; slots],
        }
    }

    pub(super) fn add(&mut self, layout: &Layout, value: u64, bytes: u64) {
        let slot = layout
            .bounds
            .iter()
            .position(|bound| value <= *bound)
            .unwrap_or(layout.bounds.len());
        if let (Some(count), Some(total)) = (self.counts.get_mut(slot), self.bytes.get_mut(slot)) {
            *count = count.saturating_add(1);
            *total = total.saturating_add(bytes);
        }
    }

    pub fn counts(&self) -> &[u64] {
        &self.counts
    }

    pub(super) fn reports(&self, layout: &Layout) -> Vec<Report> {
        layout
            .labels
            .iter()
            .zip(self.counts.iter().zip(&self.bytes))
            .map(|(bucket, (files, bytes))| Report::Bucket {
                group: layout.group,
                bucket: *bucket,
                files: *files,
                bytes: *bytes,
            })
            .collect()
    }
}
