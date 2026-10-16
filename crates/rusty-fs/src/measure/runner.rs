use std::sync::Arc;

use rusty_core::Location;

use super::profile::{ProfileSink, folder_profile};
use crate::{IoQueue, JobSlot, OwnerGuard, Priority, Provider, volume_of};

#[must_use = "dropping the handle cancels the profile"]
pub struct ProfileHandle {
    _owner: OwnerGuard,
}

pub fn spawn_profile(
    queue: &IoQueue,
    provider: Arc<dyn Provider>,
    folder: Location,
    mut sink: impl ProfileSink + Send + 'static,
) -> ProfileHandle {
    let owner = queue.owner();
    let slot = JobSlot {
        volume: volume_of(&folder),
        priority: Priority::Idle,
        owner: owner.id(),
    };
    queue.submit(
        slot,
        Box::new(move |cancel| {
            folder_profile(provider.as_ref(), &folder, cancel, &mut sink);
        }),
    );
    ProfileHandle { _owner: owner }
}
