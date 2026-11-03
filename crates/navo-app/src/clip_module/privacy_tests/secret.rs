use std::sync::Arc;
use std::time::Duration;

use navo_clipboard::{FailKind, How, Saved, SavedKind, Scope, SecretBytes};

use super::super::tests::{Rig, XorSealer, on};
use super::{Seen, Spy, assert_clean, finish, stored_bytes};

const SECRET: &str = "SECRET-4d81e2-clip";
const WAIT: Duration = Duration::from_secs(5);

fn item(id: i64, title: &str) -> Saved {
    Saved {
        id,
        kind: SavedKind::Secret,
        title: title.to_owned(),
        body: None,
        user: Some("alice".to_owned()),
        sealed: None,
        scope: Scope::Global,
        created_ms: 1,
        updated_ms: 1,
    }
}

fn plain() -> Option<SecretBytes> {
    Some(SecretBytes::from(SECRET.to_owned()))
}

#[test]
fn privacy_secret_flow_leaves_no_trace() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::with_sealer(Arc::new(XorSealer));
    let mut seen = Seen::default();
    rig.apply(on());
    let service = rig.module.peek().live.unwrap().0;
    let id = service
        .save(item(0, "bank login"), plain())
        .recv_timeout(WAIT);
    let id = id.unwrap().unwrap();
    assert_clean(SECRET, &stored_bytes(&rig));
    let revealed = service.reveal(id).recv_timeout(WAIT).unwrap().unwrap();
    assert_eq!(revealed.as_str(), Some(SECRET), "the sealer round-trips");
    seen.note("revealed", &revealed);
    drop(revealed);
    service
        .copy_saved(id, How::Copy)
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    assert_eq!(*rig.sink.privates.lock().unwrap(), [SECRET]);
    assert!(
        rig.sink.texts.lock().unwrap().is_empty(),
        "never plain text"
    );
    service
        .update(item(id, "bank login 2"), None)
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    assert_clean(SECRET, &stored_bytes(&rig));
    seen.service(&rig);
    service
        .delete_saved(id)
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    rig.module.stop();
    assert_clean(SECRET, &stored_bytes(&rig));
    finish(&spy, &seen, SECRET);
}

#[test]
fn privacy_secret_without_a_sealer_fails_without_the_text() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    rig.apply(on());
    let service = rig.module.peek().live.unwrap().0;
    let refused = service
        .save(item(0, "bank login"), plain())
        .recv_timeout(WAIT);
    assert_eq!(refused.unwrap(), Err(FailKind::SecretsUnsupported));
    seen.note("refusal", &refused);
    seen.service(&rig);
    rig.module.stop();
    assert_clean(SECRET, &stored_bytes(&rig));
    finish(&spy, &seen, SECRET);
}
