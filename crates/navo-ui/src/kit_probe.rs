//! Events kit widgets send to the host's latency recorder, without the kit knowing the recorder.
//! The host installs one function with `set_probe`; widgets call `emit`.

/// What a kit widget reports for the host's latency recorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    MenuReady,
    MenuAnimating(bool),
    MenuRendered,
}

static HOOK: std::sync::OnceLock<fn(Probe)> = std::sync::OnceLock::new();

/// Installs the host's probe once; later calls are ignored.
pub fn set_probe(hook: fn(Probe)) {
    let _ = HOOK.set(hook);
}

/// Sends one event to the host's probe; does nothing when none is installed.
pub(crate) fn emit(event: Probe) {
    if let Some(hook) = HOOK.get() {
        hook(event);
    }
}
