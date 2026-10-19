use rusty_core::{Location, OpKind};
use rusty_fs::volume_of;

use super::work::lands_inside;
use super::{Browser, OpRequest};

impl Browser {
    pub fn paste(&mut self, kind: OpKind, sources: Vec<Location>) -> Option<u64> {
        let tab = self.state.active_tab();
        let target = tab.location().clone();
        let source_tab = tab.id();
        let lands_badly = sources.iter().any(|source| {
            source
                .parent()
                .is_some_and(|folder| lands_inside(&target, &folder, &sources))
        });
        if sources.is_empty() || lands_badly {
            return None;
        }
        let same_volume = sources
            .first()
            .is_some_and(|source| volume_of(source) == volume_of(&target));
        self.next_op += 1;
        let request = OpRequest {
            id: self.next_op,
            kind,
            sources,
            target: Some(target),
            same_volume,
            protected: self.operations.protected.clone(),
            source_tab,
            asks: false,
        };
        let id = request.id;
        self.submit_plan(request).then_some(id)
    }
}
