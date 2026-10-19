use super::*;

fn session() -> SessionState {
    AppState::new(Location::new("/home").unwrap()).to_session(10)
}

#[test]
fn placement_round_trips_through_postcard_and_app_state() {
    let mut session = session();
    let current = single();
    session.windows[0].remember(&current, placement(&current, 0));
    let bytes = postcard::to_stdvec(&session).unwrap();
    let decoded: SessionState = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, session);
    assert_eq!(
        AppState::from_session(decoded).unwrap().to_session(10),
        session
    );
}

#[test]
fn an_old_named_session_defaults_to_no_placements() {
    let mut old = serde_json::to_value(session()).unwrap();
    old["windows"][0]
        .as_object_mut()
        .unwrap()
        .remove("placement");
    let loaded: SessionState = serde_json::from_value(old).unwrap();
    insta::assert_debug_snapshot!(loaded.windows[0].placement, @"[]");
}

#[test]
fn remember_caps_at_eight_and_replaces_the_same_setup_at_the_front() {
    let mut session = session();
    let window = &mut session.windows[0];
    let setups: Vec<_> = (0..10)
        .map(|i| setup(vec![monitor(physical(i * 100, 0, 1920, 1080), 1.0)]))
        .collect();
    for current in &setups {
        window.remember(current, placement(current, 0));
    }
    assert_eq!(window.placement.len(), 8);
    assert_eq!(window.placement[7].setup, setups[2].fingerprint());
    let mut updated = placement(&setups[4], 0);
    updated.maximized = true;
    window.remember(&setups[4], updated.clone());
    assert_eq!(window.placement[0], updated);
    assert_eq!(window.placement[1].setup, setups[9].fingerprint());
    assert_eq!(window.placement.len(), 8);
    assert_eq!(
        window
            .placement
            .iter()
            .filter(|p| p.setup == updated.setup)
            .count(),
        1
    );
}

#[test]
fn loading_overlong_placement_history_keeps_the_eight_newest_entries() {
    let mut session = session();
    session.windows[0].placement = (0..12)
        .map(|i| {
            let current = setup(vec![monitor(physical(i * 100, 0, 1920, 1080), 1.0)]);
            placement(&current, 0)
        })
        .collect();
    let bytes = postcard::to_stdvec(&session).unwrap();
    let decoded: SessionState = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(
        decoded.windows[0].placement,
        session.windows[0].placement[..8]
    );
}
