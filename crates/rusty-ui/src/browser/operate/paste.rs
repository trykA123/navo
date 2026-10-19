use rusty_core::{Location, OpKind};
use rusty_fs::volume_of;

use super::{Browser, OpRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteRefusal {
    Nothing,
    AlreadyHere,
    IntoItself,
}

fn refusal(target: &Location, sources: &[Location]) -> Option<PasteRefusal> {
    if sources.is_empty() {
        return Some(PasteRefusal::Nothing);
    }
    if sources
        .iter()
        .any(|source| source.parent().as_ref() == Some(target))
    {
        return Some(PasteRefusal::AlreadyHere);
    }
    let mut current = Some(target.clone());
    while let Some(location) = current {
        if sources.contains(&location) {
            return Some(PasteRefusal::IntoItself);
        }
        current = location.parent();
    }
    None
}

impl Browser {
    pub fn paste(&mut self, kind: OpKind, sources: Vec<Location>) -> Result<u64, PasteRefusal> {
        let tab = self.state.active_tab();
        let target = tab.location().clone();
        let source_tab = tab.id();
        if let Some(refused) = refusal(&target, &sources) {
            return Err(refused);
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
        if self.submit_plan(request) {
            Ok(id)
        } else {
            Err(PasteRefusal::Nothing)
        }
    }
}
