use super::*;

mod case_twins;
mod conflicts;
mod copy;
mod delete;
mod keep;
mod link;
mod move_;
mod planned;
mod props;
mod risks;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn scanned(path: &str, depth: u32, kind: ItemKind, size: u64) -> ScannedItem {
    ScannedItem {
        location: loc(path),
        depth,
        kind,
        size,
        modified_ms: Some(1_000),
    }
}

fn file(path: &str, depth: u32, size: u64) -> ScannedItem {
    scanned(path, depth, ItemKind::File, size)
}

fn folder(path: &str, depth: u32) -> ScannedItem {
    scanned(path, depth, ItemKind::Folder, 0)
}

fn link(path: &str, depth: u32) -> ScannedItem {
    scanned(path, depth, ItemKind::Link, 0)
}

fn existing(path: &str, kind: ItemKind, modified_ms: i64) -> ExistingItem {
    ExistingItem {
        location: loc(path),
        kind,
        size: 1,
        modified_ms: Some(modified_ms),
    }
}

fn request(kind: OpKind, items: Vec<ScannedItem>, target: Option<&str>) -> PlanRequest {
    PlanRequest {
        kind,
        items,
        target: target.map(loc),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: false,
        protected: Vec::new(),
    }
}

fn copy_into(items: Vec<ScannedItem>, target: &str) -> PlanRequest {
    request(OpKind::Copy, items, Some(target))
}

fn make_folder(target: &str) -> Step {
    Step::MakeFolder {
        target: loc(target),
    }
}

fn copy_file(source: &str, target: &str, size: u64) -> Step {
    Step::CopyFile {
        source: loc(source),
        target: loc(target),
        size,
    }
}

fn remove_file(path: &str) -> Step {
    Step::RemoveFile {
        location: loc(path),
    }
}

fn remove_link(path: &str) -> Step {
    Step::RemoveLink {
        location: loc(path),
    }
}

fn remove_folder(path: &str) -> Step {
    Step::RemoveFolder {
        location: loc(path),
    }
}
