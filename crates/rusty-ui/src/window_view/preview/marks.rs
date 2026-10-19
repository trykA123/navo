use std::collections::HashMap;
use std::sync::Arc;

use rusty_core::Location;
use rusty_fs::{JobSlot, OwnerGuard, Priority, Provider, folder_size, volume_of};

use super::inbox::Arrival;
use super::{Preview, Shown};
use crate::format;
use crate::prelude::*;
use crate::window_view::FerriteWindow;
use crate::window_view::viewer::PageDown;

struct Counting {
    location: Location,
    _owner: OwnerGuard,
}

#[derive(Default)]
pub(in crate::window_view) struct Marks {
    marked: Vec<Location>,
    sizes: HashMap<Location, u64>,
    counting: Vec<Counting>,
}

impl Marks {
    pub(in crate::window_view) fn len(&self) -> usize {
        self.marked.len()
    }

    pub(in crate::window_view) fn is_marked(&self, location: &Location) -> bool {
        self.marked.contains(location)
    }

    pub(in crate::window_view) fn size(&self, location: &Location) -> Option<u64> {
        self.sizes.get(location).copied()
    }

    #[cfg(test)]
    pub(in crate::window_view) fn is_counting(&self, location: &Location) -> bool {
        self.counting.iter().any(|c| &c.location == location)
    }

    pub(in crate::window_view) fn total(&self) -> Option<u64> {
        self.marked.iter().map(|location| self.size(location)).sum()
    }

    fn toggle(&mut self, location: &Location) -> bool {
        let before = self.marked.len();
        self.marked.retain(|marked| marked != location);
        if self.marked.len() < before {
            self.counting
                .retain(|counting| &counting.location != location);
            return false;
        }
        self.marked.push(location.clone());
        true
    }

    fn know(&mut self, location: &Location, bytes: u64) {
        self.sizes.insert(location.clone(), bytes);
        self.counting
            .retain(|counting| &counting.location != location);
    }

    fn needs_count(&self, location: &Location) -> bool {
        !self.sizes.contains_key(location) && !self.counting.iter().any(|c| &c.location == location)
    }

    pub(super) fn summary(&self) -> Option<String> {
        let count = self.len();
        if count == 0 {
            return None;
        }
        let known: u64 = self.marked.iter().filter_map(|l| self.size(l)).sum();
        let size = format::size(known);
        Some(match self.total() {
            Some(_) => format!("{count} marked · {size}"),
            None => format!("{count} marked · {size} so far"),
        })
    }
}

impl Preview {
    fn count(&self, provider: Arc<dyn Provider>, location: Location) -> Counting {
        let owner = self.scans.owner();
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Idle,
            owner: owner.id(),
        };
        let (send, wake) = (self.inbox.send.clone(), Arc::clone(&self.inbox.wake));
        let folder = location.clone();
        self.scans.submit(
            slot,
            Box::new(move |cancel| {
                let Ok(size) = folder_size(provider.as_ref(), &folder, cancel) else {
                    return;
                };
                let arrival = Arrival::Counted {
                    location: folder,
                    bytes: size.bytes,
                };
                if send.send(arrival).is_ok() {
                    wake();
                }
            }),
        );
        Counting {
            location,
            _owner: owner,
        }
    }

    pub(super) fn clear_marks(&mut self) {
        if let Some(folder) = self.folder_mut() {
            folder.marks = Marks::default();
        }
    }
}

impl FerriteWindow {
    pub(super) fn browse_mark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.preview.shown, Some(Shown::File(_))) {
            window.dispatch_action(Box::new(PageDown), cx);
            return;
        }
        let Some(target) = self.preview.folder().and_then(|f| f.spot_target()) else {
            self.browse_step(1, cx);
            return;
        };
        let location = target.location.clone();
        let known = if target.is_folder() {
            self.browser
                .profiles()
                .get(&location, target.changed_ms())
                .map(|profile| profile.totals.bytes)
        } else {
            Some(target.entry.size)
        };
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        let is_marked = folder.marks.toggle(&location);
        if let Some(bytes) = known {
            folder.marks.know(&location, bytes);
        } else if is_marked && folder.marks.needs_count(&location) {
            let counting = self.preview.count(self.provider_for_preview(), location);
            if let Some(folder) = self.preview.folder_mut() {
                folder.marks.counting.push(counting);
            }
        }
        self.browse_step(1, cx);
    }

    pub(super) fn folder_counted(&mut self, location: &Location, bytes: u64) -> bool {
        let preview = &mut self.preview;
        let shown = match &mut preview.shown {
            Some(Shown::Folder(folder)) => Some(&mut folder.marks),
            _ => None,
        };
        let waiting = shown
            .into_iter()
            .chain(preview.trail.iter_mut().map(|crumb| &mut crumb.marks))
            .find(|marks| marks.counting.iter().any(|c| &c.location == location));
        let Some(marks) = waiting else {
            return false;
        };
        marks.know(location, bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use rusty_core::Location;

    use super::Marks;
    use crate::format;

    fn place(path: &str) -> Location {
        Location::new(path).unwrap()
    }

    #[test]
    fn the_summary_adds_known_sizes_and_says_when_a_count_is_still_running() {
        let mut marks = Marks::default();
        assert_eq!(marks.summary(), None);
        let (notes, photos) = (place("/work/notes.txt"), place("/work/photos"));
        marks.toggle(&notes);
        marks.know(&notes, 2_048);
        marks.toggle(&photos);
        let size = format::size(2_048);
        assert_eq!(marks.summary(), Some(format!("2 marked · {size} so far")));
        marks.know(&photos, 1_024);
        let size = format::size(3_072);
        assert_eq!(marks.summary(), Some(format!("2 marked · {size}")));
        assert_eq!(marks.total(), Some(3_072));
    }

    #[test]
    fn a_second_toggle_unmarks_and_keeps_the_known_size() {
        let mut marks = Marks::default();
        let photos = place("/work/photos");
        assert!(marks.toggle(&photos));
        marks.know(&photos, 10);
        assert!(!marks.toggle(&photos));
        assert!(!marks.is_marked(&photos));
        assert_eq!(marks.size(&photos), Some(10));
        assert_eq!(marks.len(), 0);
    }
}
