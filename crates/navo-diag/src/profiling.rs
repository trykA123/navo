use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

pub fn frame_mark() {
    #[cfg(feature = "tracy")]
    tracy_client::frame_mark();
}

#[must_use]
pub fn tracy_layer<S>() -> Option<impl Layer<S>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    #[cfg(feature = "tracy")]
    {
        use tracing_subscriber::filter::LevelFilter;
        tracy_client::Client::start();
        Some(tracing_tracy::TracyLayer::default().with_filter(LevelFilter::DEBUG))
    }
    #[cfg(not(feature = "tracy"))]
    {
        None::<tracing_subscriber::layer::Identity>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::Registry;

    #[test]
    fn frame_mark_is_callable() {
        frame_mark();
    }

    #[cfg(not(feature = "tracy"))]
    #[test]
    fn tracy_layer_is_none_without_the_feature() {
        assert!(tracy_layer::<Registry>().is_none());
    }
}
