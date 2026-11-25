use std::time::Instant;

use super::*;
use crate::{CopiedSource, EntryBatch, FakeFs};

#[test]
fn listing_waits_for_configured_delay() {
    let fake = Arc::new(FakeFs::new());
    fake.add_file("folder/file", 42);
    let folder = fake.location("folder");
    let delay = Duration::from_millis(30);
    let slow = SlowFs::new(fake, delay);
    let mut entries = Vec::new();
    let started = Instant::now();

    slow.list(
        &folder,
        &mut |batch: EntryBatch| {
            entries.extend(batch.entries);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();

    assert!(started.elapsed() >= delay);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name.to_text(), "file");
    assert_eq!(entries[0].size, 42);
}

#[test]
fn pure_queries_delegate_unchanged() {
    let fake = Arc::new(FakeFs::new());
    let slow = SlowFs::new(fake.clone(), Duration::from_millis(30));

    assert_eq!(slow.id(), fake.id());
    assert_eq!(slow.capabilities(), fake.capabilities());
    assert_eq!(slow.supports_atomic_rename(), fake.supports_atomic_rename());
}

#[test]
fn invalid_or_zero_delay_disables_simulation() {
    for value in [
        None,
        Some("0"),
        Some("garbage"),
        Some(""),
        Some("-1"),
        Some("18446744073709551616"),
    ] {
        assert_eq!(SlowFs::parse_delay_ms(value), None, "{value:?}");
    }
}

#[test]
fn positive_delay_enables_simulation() {
    assert_eq!(
        SlowFs::parse_delay_ms(Some("30")),
        Some(Duration::from_millis(30))
    );
}

#[test]
fn content_hash_reaches_the_inner_provider() {
    let fake = Arc::new(FakeFs::new());
    fake.add_file("folder/file", 42);
    fake.set_hash("folder/file", [7; 32]);
    let file = fake.location("folder/file");
    let slow = SlowFs::new(fake, Duration::from_millis(1));

    assert_eq!(
        slow.content_hash(&file, &CancelToken::new()).unwrap(),
        [7; 32]
    );
}

/// Delegates to a `FakeFs`, but reads a moved source by path with creation time and attributes,
/// as `LocalFs` does on Windows.
struct ProofSpy {
    fs: FakeFs,
    seen: CopiedSource,
}

impl Provider for ProofSpy {
    fn id(&self) -> navo_core::ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> crate::Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn crate::ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<crate::FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for ProofSpy {
    fn copied_source_at(&self, _: &Location) -> Result<Option<CopiedSource>, FsError> {
        Ok(Some(self.seen))
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

#[test]
fn a_no_id_source_proof_is_read_by_path_through_the_inner_provider() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let location = fs.location("home/a");
    let proof = CopiedSource {
        identity: None,
        size: 7,
        modified_ms: Some(9),
        created_ms: Some(4),
        attributes: Some(0x20),
    };
    let slow = SlowFs::new(Arc::new(ProofSpy { fs, seen: proof }), Duration::ZERO);
    assert_eq!(slow.copied_source_at(&location).unwrap(), Some(proof));
    assert!(proof.still_at(&slow, &location).unwrap());
}
