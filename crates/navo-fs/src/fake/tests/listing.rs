use super::*;

#[test]
fn children_come_in_insertion_order() {
    let fs = FakeFs::new();
    fs.add_folder("d");
    for name in ["zeta", "alpha", "Mid", "beta"] {
        fs.add_file(&format!("d/{name}"), 1);
    }
    assert_eq!(
        names(&list_all(&fs, "d").1),
        ["zeta", "alpha", "Mid", "beta"]
    );
}

#[test]
fn listing_a_folder_succeeds() {
    assert_eq!(list_all(&with_files(2), "d").0, Ok(()));
}

#[test]
fn ten_entries_in_batches_of_three_make_four_batches() {
    let fs = with_files(10).with_batch_size(3);
    assert_eq!(
        shape(&list_all(&fs, "d").1),
        [(3, false), (3, false), (3, false), (1, true)]
    );
}

#[test]
fn batches_carry_every_entry_once() {
    let fs = with_files(10).with_batch_size(3);
    assert_eq!(names(&list_all(&fs, "d").1), file_names(10));
}

#[test]
fn default_batch_size_is_512() {
    let fs = with_files(0);
    for i in 0..513 {
        fs.add_file(&format!("d/g{i}"), 1);
    }
    assert_eq!(shape(&list_all(&fs, "d").1), [(512, false), (1, true)]);
}

#[test]
fn batch_size_zero_acts_as_one() {
    let fs = with_files(2).with_batch_size(0);
    assert_eq!(shape(&list_all(&fs, "d").1), [(1, false), (1, true)]);
}

#[test]
fn empty_folder_sends_one_empty_last_batch() {
    assert_eq!(shape(&list_all(&with_files(0), "d").1), [(0, true)]);
}

#[test]
fn root_lists_its_children() {
    let fs = FakeFs::new();
    fs.add_folder("a");
    fs.add_folder("b");
    assert_eq!(names(&list_all(&fs, "").1), ["a", "b"]);
}

#[test]
fn listing_a_file_is_name_invalid() {
    let fs = with_files(1);
    assert_eq!(
        kind_of(list_all(&fs, "d/f00").0),
        Some(FsErrorKind::NameInvalid)
    );
}

#[test]
fn listing_a_missing_folder_is_not_found() {
    assert_eq!(
        kind_of(list_all(&FakeFs::new(), "nope").0),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn listing_error_carries_the_location() {
    let fs = FakeFs::new();
    let location = fs.location("nope");
    assert_eq!(
        list_at(&fs, &location).0.err().and_then(|e| e.location),
        Some(location)
    );
}

#[test]
fn listing_another_provider_is_not_found() {
    let fs = with_files(1);
    let other = Location::new("other://d").unwrap();
    assert_eq!(kind_of(list_at(&fs, &other).0), Some(FsErrorKind::NotFound));
}

fn list_with(
    fs: &FakeFs,
    cancel: &CancelToken,
    mut on_batch: impl FnMut() -> ControlFlow<()>,
) -> (Result<(), FsError>, usize) {
    let mut count = 0;
    let mut sink = |_: EntryBatch| {
        count += 1;
        on_batch()
    };
    let result = fs.list(&fs.location("d"), &mut sink, cancel);
    (result, count)
}

#[test]
fn break_stops_with_ok() {
    let fs = with_files(10).with_batch_size(3);
    let (result, _) = list_with(&fs, &CancelToken::new(), || ControlFlow::Break(()));
    assert_eq!(result, Ok(()));
}

#[test]
fn break_stops_after_the_first_batch() {
    let fs = with_files(10).with_batch_size(3);
    let (_, count) = list_with(&fs, &CancelToken::new(), || ControlFlow::Break(()));
    assert_eq!(count, 1);
}

#[test]
fn cancel_before_the_call_is_cancelled() {
    let fs = with_files(10).with_batch_size(3);
    let cancel = CancelToken::new();
    cancel.cancel();
    let (result, _) = list_with(&fs, &cancel, || ControlFlow::Continue(()));
    assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
}

#[test]
fn cancel_in_the_sink_is_cancelled() {
    let fs = with_files(10).with_batch_size(3);
    let cancel = CancelToken::new();
    let inner = cancel.clone();
    let (result, _) = list_with(&fs, &cancel, || {
        inner.cancel();
        ControlFlow::Continue(())
    });
    assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
}

#[test]
fn cancel_in_the_sink_sends_no_further_batch() {
    let fs = with_files(10).with_batch_size(3);
    let cancel = CancelToken::new();
    let inner = cancel.clone();
    let (_, count) = list_with(&fs, &cancel, || {
        inner.cancel();
        ControlFlow::Continue(())
    });
    assert_eq!(count, 1);
}

#[test]
fn cancel_during_the_last_batch_is_ok() {
    let fs = with_files(2).with_batch_size(3);
    let cancel = CancelToken::new();
    let inner = cancel.clone();
    let (result, count) = list_with(&fs, &cancel, || {
        inner.cancel();
        ControlFlow::Continue(())
    });
    assert_eq!((result, count), (Ok(()), 1));
}

struct ListOnly(FakeFs);

impl Provider for ListOnly {
    fn id(&self) -> ProviderId {
        self.0.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.0.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.0.metadata(location)
    }
}

#[test]
fn default_list_sizes_preserves_every_entry_without_extra_metadata() {
    let fs = ListOnly(with_files(5).with_batch_size(2));
    fs.0.add_folder("d/sub");
    let expected: Vec<_> = list_all(&fs.0, "d")
        .1
        .into_iter()
        .flat_map(|batch| batch.entries)
        .map(|entry| crate::SizeEntry {
            name: entry.name,
            kind: entry.kind,
            size: entry.size,
            on_disk: None,
            link: None,
        })
        .collect();
    let mut actual = Vec::new();
    let provider: &dyn Provider = &fs;
    provider
        .list_sizes(
            &fs.0.location("d"),
            &mut |entry| {
                actual.push(entry);
                ControlFlow::Continue(())
            },
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn default_list_sizes_cancel_stops_at_the_same_batch_as_list() {
    let fs = ListOnly(with_files(5).with_batch_size(2));
    let cancel = CancelToken::new();
    let (expected_result, _) = list_with(&fs.0, &cancel, || {
        cancel.cancel();
        ControlFlow::Continue(())
    });
    let cancel = CancelToken::new();
    let mut count = 0;
    let result = fs.list_sizes(
        &fs.0.location("d"),
        &mut |_| {
            count += 1;
            cancel.cancel();
            ControlFlow::Continue(())
        },
        &cancel,
    );
    assert_eq!(result, expected_result);
    assert_eq!(count, 2);
}

#[test]
fn default_list_sizes_sink_break_stops_with_ok() {
    let fs = ListOnly(with_files(5).with_batch_size(2));
    let mut count = 0;
    let result = fs.list_sizes(
        &fs.0.location("d"),
        &mut |_| {
            count += 1;
            ControlFlow::Break(())
        },
        &CancelToken::new(),
    );
    assert_eq!(result, Ok(()));
    assert_eq!(count, 1);
}

#[test]
fn fake_list_sizes_reports_optional_disk_sizes_and_link_keys() {
    let fs = with_files(3).with_batch_size(2);
    let key = crate::LinkKey(u128::from(u64::MAX) + 1, 7);
    fs.set_on_disk("/d/f00", 4096);
    fs.set_link("d/f00", key);
    fs.set_on_disk("d/f01", 0);
    fs.set_link("d/f01", key);
    let mut entries = Vec::new();
    fs.list_sizes(
        &fs.location("d"),
        &mut |entry| {
            entries.push(entry);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| (
                entry.name.to_text(),
                entry.kind,
                entry.size,
                entry.on_disk,
                entry.link
            ))
            .collect::<Vec<_>>(),
        [
            ("f00".to_owned(), EntryKind::File, 1, Some(4096), Some(key)),
            ("f01".to_owned(), EntryKind::File, 1, Some(0), Some(key)),
            ("f02".to_owned(), EntryKind::File, 1, None, None),
        ]
    );
}
