use super::layouts as lay;
use super::{Counts, Profile};
use crate::privacy::{Report, SafeText};
use crate::safe_text;
use crate::source::{DriveKind, FsClass};

fn measure(label: SafeText, value: u64) -> Report {
    Report::Measurement { label, value }
}

impl Profile {
    /// Every row passes through `SafeWriter`; labels are fixed text.
    pub fn reports(&self) -> Vec<Report> {
        let mut rows = self.shape_rows();
        rows.extend(entry_rows(&self.counts));
        rows.extend(kind_rows(&self.counts));
        rows.extend(self.volume_rows());
        rows.extend(self.extensions.iter().map(|mix| Report::Extension {
            extension: mix.extension.clone(),
            files: mix.files,
            bytes: mix.bytes,
        }));
        rows
    }

    fn shape_rows(&self) -> Vec<Report> {
        let folders = self.folder_files;
        let mut rows = vec![
            measure(safe_text!("folders-measured"), folders.folders),
            measure(safe_text!("files-per-folder-p50"), folders.p50),
            measure(safe_text!("files-per-folder-p95"), folders.p95),
            measure(safe_text!("files-per-folder-max"), folders.max),
            measure(safe_text!("max-depth"), self.max_depth),
        ];
        rows.extend(self.depth.reports(&lay::DEPTH));
        rows.extend(self.size.reports(&lay::SIZE));
        rows.extend(self.name_length.reports(&lay::NAME_LENGTH));
        rows.extend(self.path_length.reports(&lay::PATH_LENGTH));
        rows.extend(self.age.reports(&lay::AGE));
        rows.extend(self.repo_size.reports(&lay::REPO_SIZE));
        rows
    }

    fn volume_rows(&self) -> Vec<Report> {
        let kind = match self.volume.kind {
            DriveKind::Fixed => safe_text!("drive-fixed"),
            DriveKind::Removable => safe_text!("drive-removable"),
            DriveKind::Network => safe_text!("drive-network"),
            DriveKind::Other => safe_text!("drive-other"),
            DriveKind::Unknown => safe_text!("drive-unknown"),
        };
        let fs = match self.volume.fs {
            FsClass::Ntfs => safe_text!("fs-ntfs"),
            FsClass::Refs => safe_text!("fs-refs"),
            FsClass::Fat => safe_text!("fs-fat"),
            FsClass::ExFat => safe_text!("fs-exfat"),
            FsClass::Other => safe_text!("fs-other"),
            FsClass::Unknown => safe_text!("fs-unknown"),
        };
        vec![measure(kind, 1), measure(fs, 1)]
    }
}

fn entry_rows(c: &Counts) -> Vec<Report> {
    vec![
        measure(safe_text!("files"), c.files),
        measure(safe_text!("folders"), c.folders),
        measure(safe_text!("files-over-100-mb"), c.over_100_mb),
        measure(safe_text!("files-over-1-gb"), c.over_1_gb),
        measure(safe_text!("paths-over-260"), c.paths_over_260),
        measure(safe_text!("symlinks"), c.symlinks),
        measure(safe_text!("junctions"), c.junctions),
        measure(safe_text!("reparse-points"), c.reparse_points),
        measure(safe_text!("hidden"), c.hidden),
        measure(safe_text!("system"), c.system),
        measure(safe_text!("read-only"), c.read_only),
        measure(safe_text!("hard-linked-files"), c.hard_linked),
        measure(safe_text!("hard-link-checked"), c.hard_link_checked),
        measure(safe_text!("git-repos"), c.git_repos),
        measure(safe_text!("time-unknown"), c.time_unknown),
    ]
}

fn kind_rows(c: &Counts) -> Vec<Report> {
    vec![
        measure(safe_text!("kind-archives"), c.archives),
        measure(safe_text!("kind-images"), c.images),
        measure(safe_text!("kind-pdf"), c.pdf),
        measure(safe_text!("kind-elf"), c.elf),
        measure(safe_text!("kind-map"), c.map),
        measure(safe_text!("kind-a2l"), c.a2l),
        measure(safe_text!("kind-dbc"), c.dbc),
        measure(safe_text!("sniffed-files"), c.sniffed),
        measure(safe_text!("sniffed-text"), c.text),
        measure(safe_text!("sniffed-binary"), c.binary),
    ]
}
