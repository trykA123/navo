use gpui_kit::SharedString;
use rusty_fs::FolderSize;
use rusty_l10n::FluentArgs;

use crate::browser::SizeMode;
use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Other {
    None,
    Apparent(u64),
    OnDisk(u64),
    OnDiskUnknown,
}

/// What the size card says, before it turns into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SizeLines {
    pub bytes: u64,
    pub other: Other,
    pub links_merged: u64,
    pub links_unchecked: bool,
}

impl SizeLines {
    /// `is_measured` is false while the numbers come from the index, which has no on-disk size.
    pub(super) fn new(totals: &FolderSize, mode: SizeMode, is_measured: bool) -> Self {
        let known = is_measured && totals.on_disk_known;
        let (bytes, other) = match (mode, known) {
            (SizeMode::OnDisk, true) => (totals.on_disk, Other::Apparent(totals.bytes)),
            (SizeMode::Apparent, true) => (totals.bytes, Other::OnDisk(totals.on_disk)),
            (_, false) if is_measured => (totals.bytes, Other::OnDiskUnknown),
            (_, false) => (totals.bytes, Other::None),
        };
        Self {
            bytes,
            other,
            links_merged: totals.links_merged,
            links_unchecked: totals.links_unchecked,
        }
    }

    pub(super) fn texts(&self) -> Vec<SharedString> {
        let size_arg = |bytes: u64| {
            let mut args = FluentArgs::new();
            args.set("size", format::size(bytes));
            args
        };
        let mut texts = Vec::new();
        match self.other {
            Other::None => {}
            Other::Apparent(bytes) => texts
                .push(rusty_l10n::tr_args("preview-size-other-apparent", &size_arg(bytes)).into()),
            Other::OnDisk(bytes) => texts
                .push(rusty_l10n::tr_args("preview-size-other-on-disk", &size_arg(bytes)).into()),
            Other::OnDiskUnknown => {
                texts.push(rusty_l10n::tr("preview-size-on-disk-unknown").into())
            }
        }
        if self.links_merged > 0 {
            let mut args = FluentArgs::new();
            args.set("count", self.links_merged);
            texts.push(rusty_l10n::tr_args("preview-size-links-merged", &args).into());
        }
        if self.links_unchecked {
            texts.push(rusty_l10n::tr("preview-size-links-unchecked").into());
        }
        texts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn totals() -> FolderSize {
        FolderSize {
            bytes: 1_000,
            on_disk: 4_096,
            ..FolderSize::default()
        }
    }

    #[test]
    fn the_mode_picks_the_main_number_and_the_other_one_follows() {
        let apparent = SizeLines::new(&totals(), SizeMode::Apparent, true);
        assert_eq!(
            (apparent.bytes, apparent.other),
            (1_000, Other::OnDisk(4_096))
        );
        let on_disk = SizeLines::new(&totals(), SizeMode::OnDisk, true);
        assert_eq!(
            (on_disk.bytes, on_disk.other),
            (4_096, Other::Apparent(1_000))
        );
    }

    #[test]
    fn unknown_on_disk_falls_back_to_apparent_and_says_so() {
        let totals = FolderSize {
            on_disk_known: false,
            ..totals()
        };
        let lines = SizeLines::new(&totals, SizeMode::OnDisk, true);
        assert_eq!((lines.bytes, lines.other), (1_000, Other::OnDiskUnknown));
        assert_eq!(lines.texts().len(), 1);
    }

    #[test]
    fn index_numbers_show_apparent_without_a_note() {
        let lines = SizeLines::new(&totals(), SizeMode::OnDisk, false);
        assert_eq!((lines.bytes, lines.other), (1_000, Other::None));
        assert!(lines.texts().is_empty());
    }

    #[test]
    fn merged_links_and_the_cap_add_a_note_each() {
        let plain = SizeLines::new(&totals(), SizeMode::Apparent, true);
        assert_eq!(plain.texts().len(), 1);
        let merged = FolderSize {
            links_merged: 3,
            ..totals()
        };
        assert_eq!(
            SizeLines::new(&merged, SizeMode::Apparent, true)
                .texts()
                .len(),
            2
        );
        let capped = FolderSize {
            links_unchecked: true,
            ..totals()
        };
        assert_eq!(
            SizeLines::new(&capped, SizeMode::Apparent, true)
                .texts()
                .len(),
            2
        );
        let both = FolderSize {
            links_merged: 3,
            links_unchecked: true,
            ..totals()
        };
        assert_eq!(
            SizeLines::new(&both, SizeMode::Apparent, true)
                .texts()
                .len(),
            3
        );
    }
}
