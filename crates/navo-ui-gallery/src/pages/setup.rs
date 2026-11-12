//! Table rows for the installer pages the gallery shows (one per page and state).

use super::{Page, PageInfo, info};

/// The setup pages, in the order of their `Page` variants.
pub(super) const ALL: [PageInfo; 23] = [
    info(Page::SetupWelcome, "Setup, Welcome", "setup-welcome", false),
    info(
        Page::SetupWelcomeUpdate,
        "Setup, Welcome, update in place",
        "setup-welcome-update",
        false,
    ),
    info(Page::SetupLicence, "Setup, Licence", "setup-licence", false),
    info(
        Page::SetupLicenceAccepted,
        "Setup, Licence, accepted",
        "setup-licence-accepted",
        false,
    ),
    info(Page::SetupFolder, "Setup, Folder", "setup-folder", false),
    info(
        Page::SetupFolderLocked,
        "Setup, Folder, locked",
        "setup-folder-locked",
        false,
    ),
    info(
        Page::SetupFolderProblem,
        "Setup, Folder, problem",
        "setup-folder-problem",
        false,
    ),
    info(
        Page::SetupShortcuts,
        "Setup, Shortcuts",
        "setup-shortcuts",
        false,
    ),
    info(
        Page::SetupInstalling,
        "Setup, Installing, early",
        "setup-installing",
        false,
    ),
    info(
        Page::SetupInstallingPercent,
        "Setup, Installing, 35 %",
        "setup-installing-percent",
        false,
    ),
    info(
        Page::SetupInstallingFinalizing,
        "Setup, Installing, finishing",
        "setup-installing-finalizing",
        false,
    ),
    info(
        Page::SetupInstallingSlow,
        "Setup, Installing, after 180 s",
        "setup-installing-slow",
        false,
    ),
    info(Page::SetupFinish, "Setup, Finish", "setup-finish", false),
    info(
        Page::SetupFailedRetry,
        "Setup, Failed, retry",
        "setup-failed-retry",
        false,
    ),
    info(
        Page::SetupFailedLeftover,
        "Setup, Failed, leftover folder",
        "setup-failed-leftover",
        false,
    ),
    info(Page::SetupBlocked, "Setup, Blocked", "setup-blocked", false),
    info(
        Page::SetupBlockedRetry,
        "Setup, Blocked, retry",
        "setup-blocked-retry",
        false,
    ),
    info(Page::SetupSame, "Setup, Same version", "setup-same", false),
    info(
        Page::SetupNewer,
        "Setup, Newer installed",
        "setup-newer",
        false,
    ),
    info(
        Page::SetupAmbiguous,
        "Setup, Ambiguous",
        "setup-ambiguous",
        false,
    ),
    info(
        Page::SetupAmbiguousNotice,
        "Setup, Ambiguous, unsafe name",
        "setup-ambiguous-notice",
        false,
    ),
    info(
        Page::SetupElevated,
        "Setup, Elevated",
        "setup-elevated",
        false,
    ),
    info(
        Page::SetupNoPayload,
        "Setup, No payload",
        "setup-no-payload",
        false,
    ),
];
