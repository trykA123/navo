//! Optional icon-set download, verified install, and removal hooks for the button bar.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use navo_icons::download::{DownloadSpec, downloads, install_with, remove_with};
use navo_icons::registry::{Registry, registry};
use navo_ui::window_view::{BarSetsFn, SetAction, SetNews, SetRow};

/// Maximum size accepted when fetching a set's licence file (256 KiB).
const MAX_LICENCE_BYTES: u64 = 256 * 1024;

/// Allowlisted GitHub release-asset hosts accepted in 302 `Location` headers.
#[cfg(any(windows, test))]
const ALLOWED_ASSET_HOSTS: &[&str] = &[
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// Why a release-asset fetch failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FetchError {
    /// Transport or OS networking failure with platform error code.
    #[cfg(any(windows, test))]
    Network(u32),
    /// Unexpected HTTP status code.
    #[cfg(any(windows, test))]
    Status(u16),
    /// Response body exceeded the pinned byte cap.
    #[cfg(any(windows, test))]
    TooLarge,
    /// Missing, malformed, or non-allowlisted redirect target.
    #[cfg(any(windows, test))]
    BadRedirect,
    /// Downloads are not supported on this platform.
    #[cfg(any(not(windows), test))]
    Unsupported,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            #[cfg(any(windows, test))]
            Self::Network(code) => navo_l10n::t!("button-bar-download-network", code = *code),
            #[cfg(any(windows, test))]
            Self::Status(status) => navo_l10n::t!("button-bar-download-status", status = *status),
            #[cfg(any(windows, test))]
            Self::TooLarge => navo_l10n::tr("button-bar-download-too-large"),
            #[cfg(any(windows, test))]
            Self::BadRedirect => navo_l10n::tr("button-bar-download-bad-redirect"),
            #[cfg(any(not(windows), test))]
            Self::Unsupported => navo_l10n::tr("button-bar-download-unsupported"),
        };
        f.write_str(&text)
    }
}

/// Fetches a pinned release asset by GitHub path with a strict byte cap.
pub(crate) trait Fetch: Send + Sync {
    /// Downloads `path` from the release host, enforcing `max_bytes`.
    fn fetch_release_asset(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>, FetchError>;
}

/// Platform release-asset fetcher (WinHTTP on Windows, unsupported elsewhere).
struct PlatformFetch;

impl Fetch for PlatformFetch {
    fn fetch_release_asset(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        fetch_release_asset_platform(path, max_bytes)
    }
}

/// Runtime collaborators for handling one optional icon-set action.
pub(crate) struct SetEnv<'a> {
    pub(crate) specs: &'a [DownloadSpec],
    pub(crate) fetcher: &'a dyn Fetch,
    pub(crate) icon_sets_dir: &'a Path,
    pub(crate) registry: &'a Registry,
    pub(crate) reload: &'a dyn Fn(),
}

/// Builds the button bar's optional icon-set hook backed by the platform fetcher.
pub(crate) fn sets_hook(icon_sets_dir: PathBuf, reload: Arc<dyn Fn() + Send + Sync>) -> BarSetsFn {
    Arc::new(move |action| {
        let specs = downloads();
        let env = SetEnv {
            specs: &specs,
            fetcher: &PlatformFetch,
            icon_sets_dir: &icon_sets_dir,
            registry: registry(),
            reload: reload.as_ref(),
        };
        handle_action(action, &env)
    })
}

/// Executes one [`SetAction`] against the supplied [`SetEnv`].
pub(crate) fn handle_action(action: SetAction, env: &SetEnv<'_>) -> Result<SetNews, String> {
    match action {
        SetAction::List => Ok(list_sets(env.specs, env.registry)),
        SetAction::Get(id) => get_set(id, env),
        SetAction::Remove(id) => remove_set(id, env),
    }
}

/// Parses and validates a single `Location:` redirect header to an allowlisted GitHub asset host.
#[cfg(any(windows, test))]
pub(crate) fn parse_redirect_location(raw_headers: &str) -> Result<(String, String), FetchError> {
    let location = extract_location_header(raw_headers)?;
    let rest = strip_https_scheme(location).ok_or(FetchError::BadRedirect)?;
    let slash = rest.find('/').ok_or(FetchError::BadRedirect)?;
    let authority = &rest[..slash];
    let path = &rest[slash..];
    if path.is_empty() || path.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(FetchError::BadRedirect);
    }
    let host = parse_allowed_authority(authority)?;
    Ok((host, path.to_owned()))
}

#[cfg(any(windows, test))]
fn extract_location_header(raw_headers: &str) -> Result<&str, FetchError> {
    let mut found = None;
    for line in raw_headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("location") {
            if found.is_some() {
                return Err(FetchError::BadRedirect);
            }
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(FetchError::BadRedirect);
            }
            found = Some(trimmed);
        }
    }
    found.ok_or(FetchError::BadRedirect)
}

#[cfg(any(windows, test))]
fn strip_https_scheme(url: &str) -> Option<&str> {
    let prefix = url.get(..8)?;
    if prefix.eq_ignore_ascii_case("https://") {
        url.get(8..)
    } else {
        None
    }
}

#[cfg(any(windows, test))]
fn parse_allowed_authority(authority: &str) -> Result<String, FetchError> {
    if authority.is_empty()
        || authority.contains(['@', '\\', '?', '#'])
        || authority.bytes().any(|b| b <= 0x20 || b == 0x7f)
    {
        return Err(FetchError::BadRedirect);
    }
    let host = match authority.split_once(':') {
        Some((host, "443")) => host,
        Some(_) => return Err(FetchError::BadRedirect),
        None => authority,
    };
    let lower = host.to_ascii_lowercase();
    if ALLOWED_ASSET_HOSTS.contains(&lower.as_str()) {
        Ok(lower)
    } else {
        Err(FetchError::BadRedirect)
    }
}

fn list_sets(specs: &[DownloadSpec], reg: &Registry) -> SetNews {
    let rows = specs
        .iter()
        .map(|spec| SetRow {
            id: spec.id.clone(),
            name: set_display_name(&spec.id),
            licence: spec.licence.clone(),
            size: spec.pack_size,
            installed: reg.is_downloaded(&spec.id),
        })
        .collect();
    SetNews::Listed(rows)
}

fn get_set(id: String, env: &SetEnv<'_>) -> Result<SetNews, String> {
    let spec = env
        .specs
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| navo_l10n::t!("button-bar-download-unknown-set", id = id.as_str()))?;
    let pack = env
        .fetcher
        .fetch_release_asset(&spec.pack_path, spec.pack_size)
        .map_err(|err| err.to_string())?;
    let licence_cap = spec.licence_size.min(MAX_LICENCE_BYTES);
    let licence = env
        .fetcher
        .fetch_release_asset(&spec.licence_path, licence_cap)
        .map_err(|err| err.to_string())?;
    install_with(&pack, &licence, spec, env.icon_sets_dir, &|path, bytes| {
        navo_store::replace_atomic(path, bytes)
    })
    .map_err(|err| err.to_string())?;
    reload_downloaded(env.registry, env.icon_sets_dir);
    (env.reload)();
    Ok(SetNews::Installed(id))
}

fn remove_set(id: String, env: &SetEnv<'_>) -> Result<SetNews, String> {
    remove_with(&id, env.icon_sets_dir, &|path, bytes| {
        navo_store::replace_atomic(path, bytes)
    })
    .map_err(|err| err.to_string())?;
    reload_downloaded(env.registry, env.icon_sets_dir);
    (env.reload)();
    Ok(SetNews::Removed(id))
}

fn reload_downloaded(reg: &Registry, dir: &Path) {
    for warning in reg.load_downloaded(dir) {
        tracing::warn!(problem = %warning, "downloaded icon pack warning");
    }
}

fn set_display_name(id: &str) -> String {
    match id {
        "fluent" => "Fluent".to_owned(),
        "fluent-filled" => navo_l10n::tr("button-bar-set-fluent-filled"),
        "material" => "Material".to_owned(),
        "carbon" => "Carbon".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(windows)]
fn fetch_release_asset_platform(path: &str, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
    use std::time::{Duration, Instant};

    use sidewise_winhttp::{Exchange, Request, Target, Verb};
    use windows_sys::Win32::Foundation::ERROR_TIMEOUT;

    const GITHUB_HOST: &str = "github.com";
    const PORT: u16 = 443;
    const TIMEOUTS_MS: [i32; 4] = [10_000; 4];
    const TOTAL_DEADLINE: Duration = Duration::from_secs(180);

    let start = Instant::now();
    let user_agent = format!("Navo/{}", env!("CARGO_PKG_VERSION"));
    let release_target = Target {
        host: GITHUB_HOST,
        port: PORT,
        user_agent: &user_agent,
        timeouts_ms: TIMEOUTS_MS,
    };
    let mut exchange = Exchange::open(&release_target, Verb::Get, path, "").map_err(map_winhttp)?;
    exchange.send(&[]).map_err(map_winhttp)?;
    let status = exchange.status();
    if status != 302 {
        return Err(FetchError::Status(status));
    }
    let raw_headers = exchange.raw_headers().map_err(map_winhttp)?;
    let (host, redirect_path) = parse_redirect_location(&raw_headers)?;

    let remaining = TOTAL_DEADLINE.saturating_sub(start.elapsed());
    if remaining.is_zero() {
        return Err(FetchError::Network(ERROR_TIMEOUT));
    }
    let max_bytes = usize::try_from(max_bytes).map_err(|_| FetchError::TooLarge)?;
    let asset_target = Target {
        host: &host,
        port: PORT,
        user_agent: &user_agent,
        timeouts_ms: TIMEOUTS_MS,
    };
    let request = Request {
        path: &redirect_path,
        headers: "",
        max_bytes,
        deadline: remaining,
    };
    sidewise_winhttp::get(&asset_target, &request).map_err(map_winhttp)
}

#[cfg(windows)]
fn map_winhttp(err: sidewise_winhttp::WinHttpError) -> FetchError {
    match err {
        sidewise_winhttp::WinHttpError::Network(code) => FetchError::Network(code),
        sidewise_winhttp::WinHttpError::Status(status) => FetchError::Status(status),
        sidewise_winhttp::WinHttpError::TooLarge => FetchError::TooLarge,
    }
}

#[cfg(not(windows))]
fn fetch_release_asset_platform(_path: &str, _max_bytes: u64) -> Result<Vec<u8>, FetchError> {
    Err(FetchError::Unsupported)
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
