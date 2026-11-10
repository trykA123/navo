//! Tracy profiling for the tester trace build: a span-only zone layer and frame marks.

use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

/// Marks one rendered frame in Tracy when the client is running.
pub fn frame_mark() {
    #[cfg(feature = "tracy")]
    if tracy_client::Client::running().is_some() {
        tracy_client::frame_mark();
    }
}

/// Returns the Tracy zone layer, or `None` when the `tracy` feature is off.
#[must_use]
pub fn tracy_layer<S>() -> Option<impl Layer<S>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    #[cfg(feature = "tracy")]
    {
        use tracing_subscriber::filter::filter_fn;
        tracy_client::Client::start();
        Some(ZoneLayer.with_filter(filter_fn(is_zone)))
    }
    #[cfg(not(feature = "tracy"))]
    {
        None::<tracing_subscriber::layer::Identity>
    }
}

/// Keeps spans at DEBUG or above; events and TRACE spans never open a zone.
#[cfg(feature = "tracy")]
fn is_zone(meta: &tracing::Metadata<'_>) -> bool {
    meta.is_span() && *meta.level() <= tracing::Level::DEBUG
}

/// Returns Tracy's source location: the span name, no file or function, line 0 and no
/// callstack.
#[cfg(feature = "tracy")]
fn zone_args(
    meta: &tracing::Metadata<'_>,
) -> (Option<&'static str>, &'static str, &'static str, u32, u16) {
    (Some(meta.name()), "", "", 0, 0)
}

#[cfg(feature = "tracy")]
thread_local! {
    /// Zones open on this thread, innermost last; each closes when its span exits.
    static ZONES: std::cell::RefCell<Vec<tracy_client::Span>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Opens one Tracy zone per entered span and closes it on exit; fields are ignored.
#[cfg(feature = "tracy")]
struct ZoneLayer;

#[cfg(feature = "tracy")]
impl<S> Layer<S> for ZoneLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Pushes the zone for the entered span onto this thread's stack.
    fn on_enter(&self, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(meta) = ctx.metadata(id) else {
            return;
        };
        let Some(client) = tracy_client::Client::running() else {
            return;
        };
        let (name, function, file, line, depth) = zone_args(meta);
        let zone = client.span_alloc(name, function, file, line, depth);
        ZONES.with(|zones| zones.borrow_mut().push(zone));
    }

    /// Pops and closes the zone opened for the exited span.
    fn on_exit(&self, _id: &tracing::span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        ZONES.with(|zones| {
            zones.borrow_mut().pop();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::Registry;

    #[cfg(feature = "tracy")]
    use std::sync::{Arc, Mutex, PoisonError};
    #[cfg(feature = "tracy")]
    use tracing_subscriber::layer::SubscriberExt;

    #[cfg(feature = "tracy")]
    type ZoneArgs = (Option<&'static str>, &'static str, &'static str, u32, u16);

    /// Records the zone filter's verdict for every span and event it sees.
    #[cfg(feature = "tracy")]
    #[derive(Clone, Default)]
    struct FilterProbe {
        seen: Arc<Mutex<Vec<(&'static str, bool)>>>,
    }

    #[cfg(feature = "tracy")]
    impl<S: Subscriber> Layer<S> for FilterProbe {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            seen.push(("event", is_zone(event.metadata())));
        }

        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = attrs.metadata();
            let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            seen.push((meta.name(), is_zone(meta)));
        }
    }

    /// Captures the arguments the layer passes to Tracy for one debug span.
    #[cfg(feature = "tracy")]
    #[derive(Clone, Default)]
    struct ArgsProbe {
        args: Arc<Mutex<Option<ZoneArgs>>>,
    }

    #[cfg(feature = "tracy")]
    impl<S: Subscriber> Layer<S> for ArgsProbe {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut slot = self.args.lock().unwrap_or_else(PoisonError::into_inner);
            *slot = Some(zone_args(attrs.metadata()));
        }
    }

    #[cfg(feature = "tracy")]
    #[test]
    fn profiling_span_with_path_field_does_not_panic() {
        let subscriber = Registry::default().with(tracy_layer::<Registry>());
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug_span!("listing.load", path = %"C:\\fixture\\a.txt").in_scope(|| {});
        });
    }

    #[cfg(feature = "tracy")]
    #[test]
    fn profiling_filter_rejects_events_and_trace_spans() {
        let probe = FilterProbe::default();
        let subscriber = Registry::default().with(probe.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug_span!("listing.load").in_scope(|| {});
            tracing::trace_span!("trace.only").in_scope(|| {});
            tracing::debug!("a debug event");
        });
        let seen = probe.seen.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            *seen,
            vec![
                ("listing.load", true),
                ("trace.only", false),
                ("event", false)
            ]
        );
    }

    #[cfg(feature = "tracy")]
    #[test]
    fn profiling_zone_args_have_an_empty_location() {
        let probe = ArgsProbe::default();
        let subscriber = Registry::default().with(probe.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug_span!("listing.load").in_scope(|| {});
        });
        let args = *probe.args.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(args, Some((Some("listing.load"), "", "", 0, 0)));
    }

    #[cfg(feature = "tracy")]
    #[test]
    fn profiling_frame_mark_does_not_panic() {
        frame_mark();
    }

    #[cfg(not(feature = "tracy"))]
    #[test]
    fn profiling_layer_is_none_without_the_feature() {
        assert!(tracy_layer::<Registry>().is_none());
    }
}
