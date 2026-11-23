//! AD10b: the browser resolves the verify choice when a plan is accepted, lets the user toggle it
//! and keeps the counter the job fills.

use navo_core::{FsError, ItemKind, Location, ProviderId, ScannedItem};
use navo_fs::{
    CancelToken, EntryKind, SourceContent, SourceIdentity, SourceMetadata, SourceOps, Sources,
    TempCreated, VerifyMode,
};

use crate::browser::StreamPaste;

use super::*;

fn removable(_: &navo_core::Location) -> DriveClass {
    DriveClass::Removable
}

fn ssd(_: &navo_core::Location) -> DriveClass {
    DriveClass::Ssd
}

/// Plans a copy (or `kind`) into the other pane with `class_of` and `mode`; returns the browser.
fn planned(kind: OpKind, class_of: super::super::ClassOf, mode: VerifyMode) -> Browser {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_class_of(class_of);
    browser.set_verify_mode(mode);
    assert!(browser.prepare(kind));
    settle(&mut browser);
    browser
}

/// Says whether the pending plan has a verify counter.
fn verifies(browser: &Browser) -> bool {
    let id = browser.pending().expect("a plan").id;
    browser.verifies(id).is_some()
}

#[test]
fn an_accepted_copy_to_a_removable_drive_verifies() {
    assert!(verifies(&planned(
        OpKind::Copy,
        removable,
        VerifyMode::Auto
    )));
}

#[test]
fn an_accepted_copy_to_an_ssd_does_not_verify() {
    assert!(!verifies(&planned(OpKind::Copy, ssd, VerifyMode::Auto)));
}

#[test]
fn always_verifies_a_copy_to_an_ssd() {
    assert!(verifies(&planned(OpKind::Copy, ssd, VerifyMode::Always)));
}

#[test]
fn off_never_verifies_a_copy_to_a_removable_drive() {
    assert!(!verifies(&planned(
        OpKind::Copy,
        removable,
        VerifyMode::Off
    )));
}

#[test]
fn toggle_verify_flips_the_pending_choice() {
    let mut browser = planned(OpKind::Copy, ssd, VerifyMode::Auto);
    assert_eq!(browser.verify_mode(), VerifyMode::Auto);
    assert!(browser.can_verify());
    assert!(browser.toggle_verify());
    assert!(verifies(&browser));
    assert!(!browser.toggle_verify());
    assert!(!verifies(&browser));
}

#[test]
fn a_delete_plan_is_never_eligible() {
    let mut browser = planned(OpKind::Delete, removable, VerifyMode::Always);
    assert!(!browser.can_verify());
    assert!(!verifies(&browser));
    assert!(!browser.toggle_verify());
    assert!(!verifies(&browser));
}

#[test]
fn a_finished_job_counts_its_checked_files() {
    let mut browser = planned(OpKind::Copy, removable, VerifyMode::Auto);
    let id = browser.pending().expect("a plan").id;
    assert!(browser.confirm());
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    // FakeFs reads through the cache, so a checked copy counts as unverified, never failed.
    let count = browser.verifies(id).expect("a counter");
    assert!(count.verified() + count.unverified() > 0);
}

/// A source program that writes each pasted file itself, as a mail program does.
struct Pasted(Arc<FakeFs>);

impl SourceOps for Pasted {
    fn id(&self) -> ProviderId {
        ProviderId::new("pasted").unwrap()
    }

    fn metadata(&self, _: &Location) -> Result<SourceMetadata, FsError> {
        Ok(SourceMetadata {
            kind: EntryKind::File,
            identity: SourceIdentity::Advisory,
            modified: None,
        })
    }

    fn open_read(&self, _: &Location) -> Result<Box<dyn std::io::Read + Send>, FsError> {
        Err(FsError::new(FsErrorKind::NotSupportedYet))
    }

    fn open_into(
        &self,
        _: &Location,
        (temp, _): (&Location, TempCreated<'_>),
        _: &CancelToken,
    ) -> Result<SourceContent, FsError> {
        self.0.add_file(&temp.path().to_text(), 3);
        Ok(SourceContent::Written)
    }
}

#[test]
fn a_streamed_paste_with_verify_on_counts_its_files() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_class_of(removable);
    let source = Arc::new(Pasted(Arc::clone(&fs)));
    let item = Location::provider_root(source.id())
        .join(&navo_core::ProviderPath::from_text("mail.msg"))
        .unwrap();
    let paste = StreamPaste {
        sources: Sources::new().with(source),
        items: vec![ScannedItem {
            location: item,
            depth: 0,
            kind: ItemKind::File,
            size: 3,
            modified_ms: None,
        }],
        refused: Vec::new(),
        items_only: true,
        explain: None,
    };
    let id = browser.paste_stream(paste, fs.location("dest")).unwrap();
    settle(&mut browser);
    let counter = browser.verifies(id).expect("a counter");
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert!(exists(&fs, "dest/mail.msg"));
    // A file the source program writes itself counts as unverified.
    assert_eq!(counter.verified() + counter.unverified(), 1);
}
