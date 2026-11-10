//! Tests for Viewed marks: persistence across sessions, content keys and store errors.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use navo_store::{CacheDb, CacheError};
use navo_ui::compare_diff::{Card, StreamControl, StreamUpdate, ViewedKey};

use crate::compare::diff::stream::StreamSession;
use crate::compare::diff::tests::stream::support::{
    PairData, changed_pair, channel, items, simple_load,
};
use crate::compare::diff::viewed::{ViewedMarks, ViewedStore};

const WAIT: Duration = Duration::from_secs(20);
const SIXTY_FOUR_MIB: u64 = 64 * 1024 * 1024;

fn data(ids: &[u64], lines: usize) -> HashMap<u64, PairData> {
    ids.iter()
        .map(|id| (*id, changed_pair(*id, lines, 40)))
        .collect()
}

fn open(
    store: Arc<dyn ViewedMarks>,
    data: &HashMap<u64, PairData>,
    generation: u64,
) -> (StreamSession, Receiver<StreamUpdate>) {
    let (sink, receiver) = channel();
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, simple_load(data.clone()), store);
    session.set_items(generation, items(data, true));
    (session, receiver)
}

fn next_card(receiver: &Receiver<StreamUpdate>) -> Arc<Card> {
    loop {
        if let StreamUpdate::Card { card, .. } = receiver.recv_timeout(WAIT).unwrap() {
            return card;
        }
    }
}

fn next_viewed(receiver: &Receiver<StreamUpdate>) -> (u64, bool) {
    loop {
        if let StreamUpdate::Viewed { id, viewed } = receiver.recv_timeout(WAIT).unwrap() {
            return (id, viewed);
        }
    }
}

#[test]
fn a_mark_survives_a_new_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let store = Arc::new(ViewedStore::new(CacheDb::open(&path).unwrap().0));
    let data = data(&[1], 5);
    let (session, receiver) = open(store.clone(), &data, 1);
    let card = next_card(&receiver);
    assert!(!card.viewed);
    let key = card.viewed_key;
    session.set_viewed(1, key, true);
    assert_eq!(next_viewed(&receiver), (1, true));
    drop(session);
    let (session, receiver) = open(store, &data, 2);
    let card = next_card(&receiver);
    assert!(card.viewed);
    assert_eq!(card.viewed_key, key);
    drop(session);
}

#[test]
fn changed_content_starts_unviewed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let store = Arc::new(ViewedStore::new(CacheDb::open(&path).unwrap().0));
    let before = data(&[1], 5);
    let (session, receiver) = open(store.clone(), &before, 1);
    let card = next_card(&receiver);
    let key = card.viewed_key;
    session.set_viewed(1, key, true);
    assert_eq!(next_viewed(&receiver), (1, true));
    drop(session);
    let after = data(&[1], 6);
    let (session, receiver) = open(store, &after, 2);
    let card = next_card(&receiver);
    assert_ne!(card.viewed_key, key);
    assert!(!card.viewed);
    drop(session);
}

#[test]
fn a_store_error_leaves_cards_unviewed() {
    let pairs = data(&[1, 2], 4);
    let (session, receiver) = open(Arc::new(FailingViewed), &pairs, 1);
    for _ in 0..2 {
        assert!(!next_card(&receiver).viewed);
    }
    session.set_viewed(1, ViewedKey([1; 64]), true);
    let (id, viewed) = next_viewed(&receiver);
    assert_eq!((id, viewed), (1, true));
    drop(session);
}

/// A store that reports an error on every read and write.
struct FailingViewed;

impl ViewedMarks for FailingViewed {
    fn get(&self, _key: &[u8; 64]) -> Result<bool, CacheError> {
        Err(CacheError::Io(io::Error::other("no viewed store")))
    }

    fn set(&self, _key: &[u8; 64], _viewed: bool) -> Result<(), CacheError> {
        Err(CacheError::Io(io::Error::other("no viewed store")))
    }
}
