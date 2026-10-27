use rusty_diag::components::{Held, MemoryUse, Tally};

use super::kinds::{Kind, Sniff};

pub(in crate::window_view) static VIEWER_DOCUMENTS: Tally = Tally::new("viewer documents");
pub(in crate::window_view) static PREVIEW_BODY: Tally = Tally::new("preview body");

const PAGED_BYTES: u64 = 1 << 20;

/// Estimate: a paged view (hex, or text too large to edit) reads on demand and keeps about 1 MiB of buffers; every other kind keeps a model at least as large as the file.
fn loaded_bytes(kind: &Kind, sniff: &Sniff<'_>) -> u64 {
    let is_paged = kind.id() == "hex" || (kind.id() == "text" && !kind.editable(sniff));
    if is_paged {
        sniff.len.min(PAGED_BYTES)
    } else {
        sniff.len
    }
}

pub(in crate::window_view) fn hold(tally: &'static Tally, kind: &Kind, sniff: &Sniff<'_>) -> Held {
    tally.hold(MemoryUse {
        bytes: loaded_bytes(kind, sniff),
        items: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::super::kinds::Kinds;
    use super::*;

    fn estimate(name: &str, head: &[u8], len: u64) -> u64 {
        let sniff = Sniff { head, name, len };
        let kind = Kinds::builtin().choose(&sniff).remove(0);
        loaded_bytes(&kind, &sniff)
    }

    #[test]
    fn a_text_file_small_enough_to_edit_counts_its_whole_length() {
        assert_eq!(estimate("notes.txt", b"hello\n", 4_000), 4_000);
    }

    #[test]
    fn a_paged_text_file_counts_only_its_buffers() {
        assert_eq!(estimate("huge.log", b"hello\n", 900 << 20), PAGED_BYTES);
    }

    #[test]
    fn a_held_document_leaves_the_tally_when_dropped() {
        let sniff = Sniff {
            head: b"hello\n",
            name: "notes.txt",
            len: 10,
        };
        let kind = Kinds::builtin().choose(&sniff).remove(0);
        static TALLY: Tally = Tally::new("footprint test");
        let held = hold(&TALLY, &kind, &sniff);
        assert_eq!(
            TALLY.read(),
            MemoryUse {
                bytes: 10,
                items: 1
            }
        );
        drop(held);
        assert_eq!(TALLY.read(), MemoryUse::default());
    }
}
