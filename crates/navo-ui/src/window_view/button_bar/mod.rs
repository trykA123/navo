//! Button bar view types, edit operations, and bounded latest-wins news channel.

pub(crate) mod fit;

use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::channel::mpsc::{Receiver, Sender, channel as wake_channel};

use navo_core::Location;
use navo_icons::spec::IconSpec;

use super::UserInvocation;

/// Resolved icon ready for the button bar to draw.
pub use navo_icons::registry::Resolved as BarIcon;

/// Fluent message id for opening the button bar editor.
pub const MSG_EDIT_BUTTONS: &str = "button-bar-edit-buttons";
/// Fluent message id shown when a button targets an unknown command.
pub const MSG_UNKNOWN_COMMAND: &str = "button-bar-unknown-command";
/// Fluent message id shown when an edit races a newer button bar reload.
pub const MSG_STALE: &str = "button-bar-stale";

/// Identifies a compiled program button within a loaded bar generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramId {
    /// Generation counter incremented on each bar reload.
    pub generation: u32,
    /// Index of the program entry within that generation.
    pub index: u32,
}

/// Action triggered when a button bar item is clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    /// Built-in Navo command id.
    Navo(String),
    /// Named user command from `commands.toml`.
    User(String),
    /// Inline program button resolved by the app host.
    Program {
        /// Identifier for the compiled program entry.
        id: ProgramId,
        /// Display summary of the command line.
        line: String,
        /// Whether the program consumes pane file placeholders.
        takes_files: bool,
    },
    /// Submenu containing child button items.
    Menu(Vec<ButtonItem>),
    /// Visual separator between groups of buttons.
    Separator,
}

/// One resolved item shown in the button bar or a submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ButtonItem {
    /// Display label for the button.
    pub name: String,
    /// Resolved icon for the button.
    pub icon: BarIcon,
    /// Action executed when activated.
    pub action: ButtonAction,
    /// 1-based line number in `buttonbar.toml`.
    pub line: usize,
}

/// Position of an item in the top-level bar or inside a submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemPath {
    /// Top-level item index.
    pub index: usize,
    /// Optional child index when addressing an item inside a submenu.
    pub child: Option<usize>,
}

/// Editable action fields for inserting or updating a button bar item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftAction {
    /// Built-in Navo command id.
    Navo(String),
    /// Named user command.
    User(String),
    /// Inline program definition.
    Program {
        /// Executable path or name.
        program: String,
        /// Raw argument template string.
        args: String,
        /// Optional working directory template.
        start_in: String,
        /// Whether to hide the console window on launch.
        hidden_console: bool,
    },
    /// Visual separator.
    Separator,
    /// Empty submenu container.
    Menu,
}

/// Draft payload submitted when inserting or updating a button bar item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemDraft {
    /// Display label for the item.
    pub name: String,
    /// Action configuration.
    pub action: DraftAction,
    /// Optional icon specification.
    pub icon: Option<IconSpec>,
}

/// Mutation request sent from the UI to the button bar file store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BarEdit {
    /// Insert a new item at `at`.
    Insert {
        /// Destination path.
        at: ItemPath,
        /// Draft item to insert.
        draft: ItemDraft,
    },
    /// Replace the item at `at` with `draft`.
    Update {
        /// Target item path.
        at: ItemPath,
        /// Updated item fields.
        draft: ItemDraft,
    },
    /// Remove the item at `at`.
    Remove {
        /// Target item path.
        at: ItemPath,
    },
    /// Move an item from `from` to `to`.
    Move {
        /// Source item path.
        from: ItemPath,
        /// Destination item path.
        to: ItemPath,
    },
    /// Duplicate the item at `at`.
    Duplicate {
        /// Target item path.
        at: ItemPath,
    },
}

/// Reload update pushed from the host to the window's button bar view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ButtonBarNews {
    /// A valid bar loaded with its resolved items and any non-fatal warnings.
    Applied {
        /// Resolved top-level button items.
        items: Vec<ButtonItem>,
        /// Non-fatal parse or resolution warnings.
        warnings: Vec<String>,
    },
    /// A file error; the previous bar stays active and the message is shown.
    Invalid(String),
}

/// Launches an inline program button with the current pane invocation context.
pub type ProgramRun = Arc<dyn Fn(ProgramId, UserInvocation) + Send + Sync>;

/// Persists a button bar edit to disk, returning an error message on failure.
pub type BarEditFn = Arc<dyn Fn(BarEdit) -> Result<(), String> + Send + Sync>;

/// Undoes the last persisted button bar edit, returning an error message on failure.
pub type BarUndoFn = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Bounded latest-wins sender for pushing button bar reloads to the UI.
#[derive(Clone, Debug)]
pub struct ButtonBarSender {
    latest: Arc<Mutex<Option<ButtonBarNews>>>,
    wake: Arc<Mutex<Sender<()>>>,
}

impl ButtonBarSender {
    /// Stores `news` as the latest update and wakes the receiver if needed.
    ///
    /// Returns `false` when the receiver has been dropped.
    pub fn send(&self, news: ButtonBarNews) -> bool {
        let mut wake = self.wake.lock().unwrap_or_else(PoisonError::into_inner);
        if wake.is_closed() {
            return false;
        }
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(news);
        match wake.try_send(()) {
            Ok(()) => true,
            Err(err) if err.is_full() => true,
            Err(_) => {
                *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = None;
                false
            }
        }
    }
}

/// Bounded latest-wins receiver consumed by the window's button bar listener.
#[derive(Debug)]
pub struct ButtonBarReceiver {
    latest: Arc<Mutex<Option<ButtonBarNews>>>,
    wake: Receiver<()>,
}

impl ButtonBarReceiver {
    /// Takes the latest queued news without waiting, if any is present.
    pub fn try_recv(&mut self) -> Option<ButtonBarNews> {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Awaits the next latest-wins news item, returning `None` once all senders drop.
    pub async fn recv(&mut self) -> Option<ButtonBarNews> {
        loop {
            if let Some(news) = self.try_recv() {
                return Some(news);
            }
            self.wake.next().await?;
        }
    }
}

/// Host callbacks and news stream attached to a window for the button bar.
pub struct ButtonBarHooks {
    /// Runs an inline program button.
    pub run_program: ProgramRun,
    /// Applies a structural edit to `buttonbar.toml`.
    pub edit: BarEditFn,
    /// Undoes the most recent edit to `buttonbar.toml`.
    pub undo: BarUndoFn,
    /// Location of `buttonbar.toml`, if backed by a file.
    pub file: Option<Location>,
    /// Bounded latest-wins stream of bar reload updates.
    pub news: Option<ButtonBarReceiver>,
}

impl ButtonBarHooks {
    /// Creates paired hooks and a bounded latest-wins sender for button bar updates.
    #[must_use]
    pub fn channel(
        run_program: ProgramRun,
        edit: BarEditFn,
        undo: BarUndoFn,
        file: Option<Location>,
    ) -> (Self, ButtonBarSender) {
        let latest = Arc::new(Mutex::new(None));
        let (wake_tx, wake_rx) = wake_channel(1);
        let hooks = Self {
            run_program,
            edit,
            undo,
            file,
            news: Some(ButtonBarReceiver {
                latest: Arc::clone(&latest),
                wake: wake_rx,
            }),
        };
        let sender = ButtonBarSender {
            latest,
            wake: Arc::new(Mutex::new(wake_tx)),
        };
        (hooks, sender)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_applied(label: &str, line: usize) -> ButtonBarNews {
        ButtonBarNews::Applied {
            items: vec![ButtonItem {
                name: label.to_owned(),
                icon: BarIcon::Missing { dashed: false },
                action: ButtonAction::Navo(String::from("pane.refresh")),
                line,
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn button_bar_types_channel_delivers_news() {
        let (mut hooks, sender) = ButtonBarHooks::channel(
            Arc::new(|_, _| {}),
            Arc::new(|_| Ok(())),
            Arc::new(|| Ok(())),
            None,
        );
        let mut receiver = hooks.news.take().expect("channel populates news receiver");

        let first = sample_applied("Refresh", 1);
        assert!(sender.send(first.clone()));
        let got = futures::executor::block_on(receiver.recv());
        assert_eq!(got, Some(first));

        for idx in 1..=100 {
            assert!(sender.send(sample_applied(&format!("Item {idx}"), idx)));
        }
        let newest = futures::executor::block_on(receiver.recv());
        assert_eq!(newest, Some(sample_applied("Item 100", 100)));
        assert_eq!(receiver.try_recv(), None);

        drop(sender);
        assert_eq!(futures::executor::block_on(receiver.recv()), None);

        let (hooks2, sender2) = ButtonBarHooks::channel(
            Arc::new(|_, _| {}),
            Arc::new(|_| Ok(())),
            Arc::new(|| Ok(())),
            None,
        );
        drop(hooks2);
        assert!(!sender2.send(ButtonBarNews::Invalid(String::from("closed"))));
    }
}
