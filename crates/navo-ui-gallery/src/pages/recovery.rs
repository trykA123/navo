use super::{Page, PageInfo, info};

pub(super) const NOTICE: PageInfo = info(
    Page::RecoveryNotice,
    "Storage recovery",
    "p35-recovery",
    false,
);

pub(super) const NARROW: PageInfo = info(
    Page::RecoveryNoticeNarrow,
    "Storage recovery, 1100",
    "p35-recovery-1100",
    false,
);
