use std::sync::Arc;

use super::hand_off::RawWindowHandle;
use crate::browser::{FolderTotalsQuery, no_folder_totals};
pub use crate::settings::ServiceStatus;

pub type CommandUse = Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync>;
pub type RecentCommands = Arc<dyn Fn(usize) -> Vec<String> + Send + Sync>;
pub type RecordCommand = Arc<dyn Fn(&str) + Send + Sync>;
pub type Frecents = Arc<dyn Fn(usize) -> Vec<Frecent> + Send + Sync>;
pub type RecordVisit = Arc<dyn Fn(&str, bool) + Send + Sync>;
pub type ClearSearch = Arc<dyn Fn() + Send + Sync>;
pub type ServiceState = Arc<dyn Fn() -> ServiceStatus + Send + Sync>;
pub type ServiceAction = Arc<dyn Fn() + Send + Sync>;
pub type ServiceChange = Arc<dyn Fn() -> ServiceOutcome + Send + Sync>;
pub type UpgradeDue = Arc<dyn Fn() -> bool + Send + Sync>;

/// How an install, update or remove ended; a cancelled prompt counts as `Done`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceOutcome {
    Done,
    /// Removed; Windows deletes the last files at the next restart.
    Deferred,
    Failed,
    /// A service file is in use; the user closes the program holding it and retries.
    Locked,
    /// The service image is not the one built with this Navo; the user reinstalls Navo.
    Untrusted,
}

impl ServiceOutcome {
    /// The l10n key of the notice this outcome shows, or `None` for a plain success.
    pub fn message_key(self) -> Option<&'static str> {
        match self {
            Self::Done => None,
            Self::Deferred => Some("settings-search-service-deferred"),
            Self::Failed => Some("settings-search-service-failed"),
            Self::Locked => Some("settings-search-service-locked"),
            Self::Untrusted => Some("settings-search-service-untrusted"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frecent {
    pub location: String,
    pub is_folder: bool,
    pub weight: f64,
}

#[derive(Clone)]
pub struct ServiceHooks {
    pub status: ServiceState,
    pub install: ServiceChange,
    pub remove: ServiceChange,
    pub upgrade_due: UpgradeDue,
    pub dismiss_upgrade: ServiceAction,
}

impl ServiceHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            status: Arc::new(|| ServiceStatus::NotAvailable),
            install: Arc::new(|| ServiceOutcome::Done),
            remove: Arc::new(|| ServiceOutcome::Done),
            upgrade_due: Arc::new(|| false),
            dismiss_upgrade: Arc::new(|| {}),
        }
    }
}

/// Orders the results window above its owner (`true`) or takes that order away.
pub type WindowOrder = Arc<dyn Fn(RawWindowHandle, bool) + Send + Sync>;

/// A find the user saved: its name, query text and the folder it starts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedSearch {
    pub id: i64,
    pub name: String,
    pub query: String,
    pub base: Option<String>,
}

/// Saves a find under a name, with its query text and an optional starting folder.
type SaveSearch = Arc<dyn Fn(&str, &str, Option<&str>) -> Result<i64, String> + Send + Sync>;
/// Saves the current results under a name, with the paths they cover.
type SaveSnapshot =
    Arc<dyn Fn(&str, &str, Option<&str>, &[String]) -> Result<i64, String> + Send + Sync>;

/// The app's side of the find results window; the core UI replaces the saves with real ones.
#[derive(Clone)]
pub struct FindHooks {
    pub pin: WindowOrder,
    pub save_search: SaveSearch,
    pub save_snapshot: SaveSnapshot,
    pub saved: Arc<dyn Fn() -> Vec<SavedSearch> + Send + Sync>,
    pub delete_saved: Arc<dyn Fn(i64) -> Result<(), String> + Send + Sync>,
}

impl FindHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            pin: Arc::new(|_, _| {}),
            save_search: Arc::new(|_, _, _| Err(navo_l10n::tr("find-save-off").to_owned())),
            save_snapshot: Arc::new(|_, _, _, _| Err(navo_l10n::tr("find-save-off").to_owned())),
            saved: Arc::new(Vec::new),
            delete_saved: Arc::new(|_| Err(navo_l10n::tr("find-save-off").to_owned())),
        }
    }
}

#[derive(Clone)]
pub struct SearchHooks {
    pub index: crate::command_center::IndexHooks,
    pub command_use: CommandUse,
    pub recent_commands: RecentCommands,
    pub record_command: RecordCommand,
    pub frecent: Frecents,
    pub record_visit: RecordVisit,
    pub clear: ClearSearch,
    pub service: ServiceHooks,
    pub folder_totals: FolderTotalsQuery,
    pub find: FindHooks,
}

impl SearchHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            index: crate::command_center::IndexHooks::none(),
            command_use: Arc::new(Vec::new),
            recent_commands: Arc::new(|_| Vec::new()),
            record_command: Arc::new(|_| {}),
            frecent: Arc::new(|_| Vec::new()),
            record_visit: Arc::new(|_, _| {}),
            clear: Arc::new(|| {}),
            service: ServiceHooks::none(),
            folder_totals: no_folder_totals(),
            find: FindHooks::none(),
        }
    }
}
