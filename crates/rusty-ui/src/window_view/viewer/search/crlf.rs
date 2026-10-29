use std::sync::Arc;

use rusty_core::marks::Bookmarks;

#[derive(Clone)]
pub struct CrlfOffsets(pub Arc<str>);

fn collect(starts: impl Iterator<Item = u64>) -> Bookmarks {
    let mut starts: Vec<u64> = starts.collect();
    starts.dedup();
    let mut bookmarks = Bookmarks::default();
    for at in starts {
        bookmarks.toggle(at);
    }
    bookmarks
}

impl CrlfOffsets {
    fn newlines(&self) -> impl Iterator<Item = u64> + '_ {
        self.0
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(at, _)| at as u64)
    }

    pub(in crate::window_view::viewer) fn to_disk(&self, bookmarks: &Bookmarks) -> Bookmarks {
        let mut newlines = self.newlines().peekable();
        let mut before = 0;
        collect(bookmarks.iter().map(|at| {
            while newlines.next_if(|newline| *newline < at).is_some() {
                before += 1;
            }
            at + before
        }))
    }

    pub(in crate::window_view::viewer) fn to_buffer(&self, bookmarks: &Bookmarks) -> Bookmarks {
        let mut newlines = self.newlines().peekable();
        let mut before = 0;
        collect(bookmarks.iter().map(|at| {
            while newlines
                .next_if(|newline| newline + before + 1 < at)
                .is_some()
            {
                before += 1;
            }
            at.saturating_sub(before)
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusty_core::marks::Bookmarks;

    use super::CrlfOffsets;

    fn bookmarks(starts: &[u64]) -> Bookmarks {
        let mut bookmarks = Bookmarks::default();
        for at in starts {
            bookmarks.toggle(*at);
        }
        bookmarks
    }

    #[test]
    fn line_starts_move_by_one_byte_per_earlier_line_end() {
        let offsets = CrlfOffsets(Arc::from("one\ntwo\n\nfour\n"));
        let disk = offsets.to_disk(&bookmarks(&[0, 4, 8, 9]));
        assert_eq!(disk.iter().collect::<Vec<_>>(), [0, 5, 10, 12]);
        let buffer = offsets.to_buffer(&disk);
        assert_eq!(buffer.iter().collect::<Vec<_>>(), [0, 4, 8, 9]);
    }
}
