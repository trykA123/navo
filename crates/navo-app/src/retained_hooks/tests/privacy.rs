use super::*;
use navo_core::FsErrorKind;
use navo_fs::{FakeOp, Fault};
use std::fmt::Debug;
use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S: tracing::Subscriber> Layer<S> for Events {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

/// With one registered dispatcher, tracing decides a callsite's interest from the calling
/// thread alone, so a parallel test without a subscriber could silence the capture.
fn pin_callsite_interest() {
    static PINNED: std::sync::LazyLock<[tracing::Dispatch; 2]> = std::sync::LazyLock::new(|| {
        [(); 2].map(|()| tracing::Dispatch::new(tracing_subscriber::Registry::default()))
    });
    std::sync::LazyLock::force(&PINNED);
    tracing::callsite::rebuild_interest_cache();
}

#[test]
fn failing_restore_logs_an_entry_id_and_kind_without_paths() {
    let profile = Profile::new();
    profile.fs.add_file("secret-documents/payroll.txt", 8);
    profile.delete(&["secret-documents/payroll.txt"]);
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    let next = profile.fs.calls("secret-documents", FakeOp::Identity) + 1;
    profile.fs.fault(
        "secret-documents",
        FakeOp::Identity,
        next,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    pin_callsite_interest();
    let events = Events::default();
    let subscriber = tracing_subscriber::Registry::default().with(events.clone());
    tracing::subscriber::with_default(subscriber, || {
        assert!(hooks.restore(id).is_err());
    });
    let lines = events.0.lock().unwrap().join("\n");
    assert!(lines.contains(&format!("id={}", id.0)), "{lines}");
    assert!(lines.contains("AccessDenied"), "{lines}");
    for private in [
        "secret-documents",
        "payroll.txt",
        profile.dirs.data.to_str().unwrap(),
    ] {
        assert!(!lines.contains(private), "{lines}");
    }
}
