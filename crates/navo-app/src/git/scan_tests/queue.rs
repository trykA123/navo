use super::*;

fn listing(generation: u64) -> Arc<Listing> {
    let folder = Location::new("fixture").unwrap();
    let rows = vec![Row {
        name: "a".into(),
        on_screen: true,
    }];
    Listing::new(&folder, generation, rows).unwrap()
}

fn shown(shared: &Shared) -> Option<Arc<FolderBadges>> {
    let folder = Location::new("fixture").unwrap();
    shared.active.get(&folder)?.badges.clone()
}

fn published(shared: &mut Shared, first: &Arc<Listing>) -> Arc<FolderBadges> {
    shared.publish(first, FolderBadges::default()).unwrap();
    shown(shared).unwrap()
}

#[test]
fn rescan_keeps_the_badges_until_the_new_scan_publishes() {
    let mut shared = Shared::default();
    let first = listing(1);
    shared.observe(first.clone());
    let before = published(&mut shared, &first);
    shared.rescan(&Location::new("fixture").unwrap());
    assert!(Arc::ptr_eq(&shown(&shared).unwrap(), &before));
    let current = shared.active.values().next().unwrap().listing.clone();
    assert!(!Arc::ptr_eq(&current, &first));
    assert!(first.cancelled.is_cancelled());
    assert!(shared.is_current(&current));
}

#[test]
fn observing_the_same_folder_again_keeps_its_badges() {
    let mut shared = Shared::default();
    let first = listing(1);
    shared.observe(first.clone());
    let before = published(&mut shared, &first);
    shared.observe(listing(2));
    assert!(Arc::ptr_eq(&shown(&shared).unwrap(), &before));
}

#[test]
fn forgetting_the_folder_drops_its_badges() {
    let mut shared = Shared::default();
    let first = listing(1);
    shared.observe(first.clone());
    published(&mut shared, &first);
    shared.forget(&Location::new("fixture").unwrap());
    assert!(shown(&shared).is_none());
}

fn folder() -> Location {
    Location::new("fixture").unwrap()
}

fn current(shared: &Shared) -> Arc<Listing> {
    shared.active.values().next().unwrap().listing.clone()
}

#[test]
fn all_survives_a_newer_observe() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    shared.rescan(&folder());
    shared.observe(listing(2));
    assert!(shared.take_forced(&current(&shared)).all);
}

#[test]
fn a_changed_rescan_keeps_forced_roots() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    let path = PathBuf::from("/work/repo");
    shared.rescan_repo(&folder(), path.clone());
    shared.rescan_changed(&folder());
    assert_eq!(
        shared.take_forced(&current(&shared)).roots,
        vec![root_key(&path)]
    );
}

#[test]
fn take_forced_clears_the_scope() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    shared.rescan(&folder());
    let current = current(&shared);
    assert!(shared.take_forced(&current).all);
    assert_eq!(shared.take_forced(&current), Forced::default());
}

#[test]
fn a_repo_scope_forces_only_its_root() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    let path = PathBuf::from("/work/repo");
    shared.rescan_repo(&folder(), path.clone());
    let forced = shared.take_forced(&current(&shared));
    assert!(!forced.all);
    assert_eq!(forced.roots, vec![root_key(&path)]);
}

#[test]
fn a_repo_scope_becomes_all_past_the_root_cap() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    for index in 0..=REPO_CAP {
        shared.rescan_repo(&folder(), PathBuf::from(format!("/repo-{index}")));
    }
    let forced = shared.take_forced(&current(&shared));
    assert!(forced.all);
    assert!(forced.roots.is_empty());
}

#[test]
fn forgetting_the_folder_clears_the_scope() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    shared.rescan(&folder());
    shared.forget(&folder());
    shared.observe(listing(2));
    assert_eq!(shared.take_forced(&current(&shared)), Forced::default());
}

#[test]
fn a_replaced_listing_returns_the_default_scope() {
    let mut shared = Shared::default();
    shared.observe(listing(1));
    shared.rescan(&folder());
    let before = current(&shared);
    shared.take_forced(&before);
    shared.observe(listing(2));
    assert_eq!(shared.take_forced(&current(&shared)), Forced::default());
}
