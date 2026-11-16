//! Side builders per backend: the large view uses its backend's, and Navo's parts open `LocalSide`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use navo_compare::text::Options;
use navo_ui::compare_diff::{DiffUpdate, PairRequest};

use super::large::{WAIT, pair, request, sink};
use crate::compare::diff::hooks::{DiffBackend, SideFn};
use crate::compare::diff::large::LargeDiff;
use crate::compare::diff::load::LoadInput;
use crate::compare::diff::navo_backend::navo_parts;
use crate::compare::diff::pair::large_input;
use crate::compare::diff::sides::side_of;
use crate::compare::diff::viewed::MemoryViewed;
use crate::compare::dirs::CompareDirs;
use crate::compare::settings::CompareSettings;
use crate::compare::std_side::opened;

#[test]
fn the_large_view_reads_through_the_backends_side_builder() {
    let dir = tempfile::tempdir().unwrap();
    let files = pair(dir.path(), 1000, 10);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let side: SideFn = Arc::new(move |location, settings| {
        counted.fetch_add(1, Ordering::SeqCst);
        side_of(location, settings)
    });
    let backend = DiffBackend::with_parts(side, Arc::new(|| Arc::new(MemoryViewed::default())));
    let request = PairRequest {
        left: files.left,
        right: files.right,
        ..request()
    };
    let load = LoadInput {
        left: None,
        right: None,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let input = large_input(&request, &load, &backend.plan(CompareSettings::default()));
    let (sink, receive) = sink();
    let control = LargeDiff::start(request, input, sink);
    let deadline = Instant::now() + WAIT;
    while !matches!(receive.recv_timeout(WAIT), Ok(DiffUpdate::Large { .. })) {
        assert!(Instant::now() < deadline, "no large update");
    }
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "the large view did not build its sides with the backend's side builder"
    );
    drop(control);
}

#[test]
fn navos_backend_parts_open_a_local_side() {
    let dir = tempfile::tempdir().unwrap();
    let files = pair(dir.path(), 10, 3);
    let parts = navo_parts(CompareDirs::default(), None);
    opened::take();
    let built = (parts.side)(files.left.as_ref().unwrap(), &CompareSettings::default());
    assert!(built.is_ok(), "Navo's side builder failed on a local file");
    assert_eq!(opened::take(), Some("LocalSide"));
}

/// A Git `Location` in a repository that does not exist.
fn git_location() -> navo_core::Location {
    let provider = navo_core::ProviderId::new("git").unwrap();
    let path = navo_core::ProviderPath::from_text("/no/such/repo@HEAD/a.txt");
    navo_core::Location::with_provider(provider, path).unwrap()
}

/// The `Sources` a large view of `backend` gets, as `large_input` builds them.
fn sources_of(backend: &DiffBackend) -> crate::compare::diff::sources::Sources {
    let load = LoadInput {
        left: None,
        right: None,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    large_input(&request(), &load, &backend.plan(CompareSettings::default())).sources
}

#[test]
fn sources_read_git_blobs_through_the_backends_side_builder() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let side: SideFn = Arc::new(move |_location, _settings| {
        counted.fetch_add(1, Ordering::SeqCst);
        Err(navo_compare::model::Reason::NotFound)
    });
    let backend = DiffBackend::with_parts(side, Arc::new(|| Arc::new(MemoryViewed::default())));
    let opened = sources_of(&backend).open(
        &git_location(),
        &CompareSettings::default(),
        &navo_compare::model::CancelToken::new(),
    );
    assert!(opened.is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "Sources did not build the Git side with the backend's side builder"
    );
}

#[test]
fn sidewises_default_sources_refuse_git_through_std_side() {
    opened::take();
    let result = sources_of(&DiffBackend::default()).open(
        &git_location(),
        &CompareSettings::default(),
        &navo_compare::model::CancelToken::new(),
    );
    assert!(
        matches!(result, Err(navo_compare::model::Reason::Unsupported)),
        "Sidewise's default side builder (StdSide) must refuse a Git blob"
    );
    assert_ne!(opened::take(), Some("LocalSide"));
}
