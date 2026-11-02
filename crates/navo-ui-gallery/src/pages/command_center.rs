use super::{Page, PageInfo, info};

pub(super) const MIXED: PageInfo =
    info(Page::CommandCenter, "Command center", "p40-a7-mixed", false);

pub(super) const RECENT: PageInfo = info(
    Page::CommandCenterRecent,
    "Command center, recent",
    "p40-a7-recent",
    false,
);

pub(super) const COMMANDS: PageInfo = info(
    Page::CommandCenterCommands,
    "Command center, commands",
    "p40-a7-commands",
    false,
);

pub(super) const PATH: PageInfo = info(
    Page::CommandCenterPath,
    "Command center, path",
    "p40-a7-path",
    false,
);

pub(super) const ARGUMENT: PageInfo = info(
    Page::CommandCenterArgument,
    "Command center, argument",
    "p40-a7-argument",
    false,
);

pub(super) const NO_MATCH: PageInfo = info(
    Page::CommandCenterNoMatch,
    "Command center, no match",
    "p40-a7-no-match",
    false,
);

pub(super) const INDEXING: PageInfo = info(
    Page::CommandCenterIndexing,
    "Command center, indexing",
    "p40-a7-indexing",
    false,
);
