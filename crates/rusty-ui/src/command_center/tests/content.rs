use super::super::content_source::ContentQuery;

#[test]
fn filters_keep_their_labels_and_invalid_message() {
    let query = ContentQuery::parse("needle ext:rs size:>1k size:oops", false, false);
    assert_eq!(query.chips[0].label.as_ref(), "ext:rs");
    assert_eq!(query.chips[1].label.as_ref(), "size:>1k");
    assert_eq!(
        query.chips[2].message.as_deref(),
        Some("size: needs a number, like <50MB")
    );
    assert_eq!(query.find.text, "needle");
}

#[test]
fn flags_add_and_remove_their_chips() {
    let query = ContentQuery::parse("needle", true, true);
    assert!(query.find.case && query.find.regex);
    assert_eq!(query.chips.len(), 2);
    assert!(ContentQuery::parse("needle", false, false).chips.is_empty());
}

#[test]
fn preview_stops_after_twenty_hits() {
    use super::super::ContentSource;
    use super::super::content_source::PreviewEvent;
    use rusty_core::find::{FileHits, FindQuery, LineHit};
    use rusty_core::{Location, ProviderPath};
    use rusty_fs::find::{FindProgress, FoundBatch, FoundFile};
    use rusty_fs::{Attributes, CancelToken, EntryKind, FsEntry};
    use std::sync::Arc;
    use std::time::Duration;

    let source = ContentSource::with_walk(Arc::new(|_, _, _, sink| {
        let batch = FoundBatch {
            files: vec![FoundFile {
                relative: ProviderPath::from_text("a.txt"),
                entry: FsEntry {
                    name: ProviderPath::from_text("a.txt"),
                    kind: EntryKind::File,
                    size: 100,
                    modified_ms: None,
                    attributes: Attributes::default(),
                },
                hits: FileHits {
                    hits: (1..=30)
                        .map(|line| LineHit {
                            line,
                            column: 0,
                            text: "needle".into(),
                        })
                        .collect(),
                    total: 30,
                    skipped: None,
                },
            }],
            progress: FindProgress::default(),
        };
        assert!(sink(batch).is_break());
        Ok(FindProgress::default())
    }));
    let preview = source.start(
        Location::new("/tmp").unwrap(),
        FindQuery::parse("needle"),
        CancelToken::new(),
    );
    let mut count = 0;
    while let PreviewEvent::Row(_) = preview
        .receive
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
    {
        count += 1;
    }
    assert_eq!(count, 20);
}
