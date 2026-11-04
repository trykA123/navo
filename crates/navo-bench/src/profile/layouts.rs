use super::histogram::Layout;
use crate::safe_text;

pub(super) const KIB: u64 = 1024;
pub(super) const MIB: u64 = 1024 * KIB;
pub(super) const GIB: u64 = 1024 * MIB;
pub(super) const LONG_PATH: u64 = 260;
const DAY: u64 = 86_400;

pub(super) const SIZE: Layout = Layout {
    group: safe_text!("file-size"),
    bounds: &[0, 4 * KIB, 64 * KIB, MIB, 16 * MIB, 100 * MIB, GIB],
    labels: &[
        safe_text!("empty"),
        safe_text!("up-to-4k"),
        safe_text!("up-to-64k"),
        safe_text!("up-to-1m"),
        safe_text!("up-to-16m"),
        safe_text!("up-to-100m"),
        safe_text!("up-to-1g"),
        safe_text!("over-1g"),
    ],
};

pub(super) const NAME_LENGTH: Layout = Layout {
    group: safe_text!("name-length"),
    bounds: &[8, 16, 32, 64, 128],
    labels: &[
        safe_text!("up-to-8"),
        safe_text!("up-to-16"),
        safe_text!("up-to-32"),
        safe_text!("up-to-64"),
        safe_text!("up-to-128"),
        safe_text!("over-128"),
    ],
};

pub(super) const PATH_LENGTH: Layout = Layout {
    group: safe_text!("path-length"),
    bounds: &[64, 128, 200, LONG_PATH],
    labels: &[
        safe_text!("up-to-64"),
        safe_text!("up-to-128"),
        safe_text!("up-to-200"),
        safe_text!("up-to-260"),
        safe_text!("over-260"),
    ],
};

pub(super) const DEPTH: Layout = Layout {
    group: safe_text!("file-depth"),
    bounds: &[0, 1, 2, 3, 5, 8],
    labels: &[
        safe_text!("0"),
        safe_text!("1"),
        safe_text!("2"),
        safe_text!("3"),
        safe_text!("4-5"),
        safe_text!("6-8"),
        safe_text!("over-8"),
    ],
};

pub(super) const REPO_SIZE: Layout = Layout {
    group: safe_text!("git-repo-size"),
    bounds: &[MIB, 16 * MIB, 100 * MIB, GIB],
    labels: &[
        safe_text!("up-to-1m"),
        safe_text!("up-to-16m"),
        safe_text!("up-to-100m"),
        safe_text!("up-to-1g"),
        safe_text!("over-1g"),
    ],
};

pub(super) const AGE: Layout = Layout {
    group: safe_text!("changed-age"),
    bounds: &[DAY, 7 * DAY, 365 * DAY],
    labels: &[
        safe_text!("last-day"),
        safe_text!("last-week"),
        safe_text!("last-year"),
        safe_text!("older"),
    ],
};
