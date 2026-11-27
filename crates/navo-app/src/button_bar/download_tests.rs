use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use navo_icons::download::DownloadSpec;
use navo_icons::pack::{self, IconDraft, SetDraft};
use navo_icons::registry::{Registry, Resolved};
use navo_icons::spec::IconSpec;
use navo_ui::window_view::{SetAction, SetNews};
use sha2::{Digest, Sha256};

use super::{Fetch, FetchError, SetEnv, handle_action, parse_redirect_location};

const SAMPLE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;
const SAMPLE_LICENCE: &[u8] = b"MIT License\n\nPermission is hereby granted.\n";

struct FakeFetch {
    bodies: HashMap<String, Vec<u8>>,
}

impl Fetch for FakeFetch {
    fn fetch_release_asset(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let body = self.bodies.get(path).ok_or(FetchError::Status(404))?;
        if body.len() as u64 > max_bytes {
            return Err(FetchError::TooLarge);
        }
        Ok(body.clone())
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn make_pack(id: &str, icon_name: &str) -> Vec<u8> {
    let draft = SetDraft {
        id: id.to_owned(),
        version: "1.0.0".to_owned(),
        licence: "MIT".to_owned(),
        grid: 24,
        icons: vec![IconDraft {
            name: icon_name.to_owned(),
            svg: SAMPLE_SVG.to_vec(),
            tags: vec!["sample".to_owned()],
            brand: None,
        }],
    };
    pack::write(&[draft]).expect("write test pack")
}

fn make_spec(id: &str, pack_bytes: &[u8], licence_bytes: &[u8]) -> DownloadSpec {
    DownloadSpec {
        id: id.to_owned(),
        package: format!("@example/{id}"),
        version: "1.0.0".to_owned(),
        pack_path: format!("/trykA123/navo/releases/download/icon-sets-1/{id}.nvic"),
        pack_size: pack_bytes.len() as u64,
        pack_sha256: sha256(pack_bytes),
        licence: "MIT".to_owned(),
        licence_path: format!("/trykA123/navo/releases/download/icon-sets-1/{id}-LICENSE"),
        licence_size: licence_bytes.len() as u64,
        licence_sha256: sha256(licence_bytes),
    }
}

#[test]
fn download_redirect_allows_only_github_asset_hosts() {
    let ok_cases = [
        (
            "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com/github-production-release-asset/123?sig=abc\r\n\r\n",
            (
                "objects.githubusercontent.com",
                "/github-production-release-asset/123?sig=abc",
            ),
        ),
        (
            "HTTP/1.1 302 Found\r\nlocation: https://release-assets.githubusercontent.com:443/packs/fluent.nvic\r\n\r\n",
            ("release-assets.githubusercontent.com", "/packs/fluent.nvic"),
        ),
        (
            "HTTP/1.1 302 Found\r\nLOCATION: https://OBJECTS.GITHUBUSERCONTENT.COM:443/a/b\r\n\r\n",
            ("objects.githubusercontent.com", "/a/b"),
        ),
    ];
    for (headers, (want_host, want_path)) in ok_cases {
        let got = parse_redirect_location(headers).expect("allowlisted redirect must succeed");
        assert_eq!(got, (want_host.to_owned(), want_path.to_owned()));
    }

    let bad_cases = [
        // Plain HTTP
        "HTTP/1.1 302 Found\r\nLocation: http://objects.githubusercontent.com/pack.nvic\r\n\r\n",
        // Userinfo in authority
        "HTTP/1.1 302 Found\r\nLocation: https://user@objects.githubusercontent.com/pack.nvic\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com@evil.example/pack.nvic\r\n\r\n",
        // Foreign host
        "HTTP/1.1 302 Found\r\nLocation: https://evil.example/pack.nvic\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com.evil.example/pack.nvic\r\n\r\n",
        // Non-443 port
        "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com:8443/pack.nvic\r\n\r\n",
        // Missing Location header
        "HTTP/1.1 302 Found\r\nContent-Length: 0\r\n\r\n",
        // Missing path
        "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com\r\n\r\n",
        // Duplicate Location headers
        "HTTP/1.1 302 Found\r\nLocation: https://objects.githubusercontent.com/a\r\nLocation: https://objects.githubusercontent.com/b\r\n\r\n",
    ];
    for headers in bad_cases {
        assert_eq!(
            parse_redirect_location(headers),
            Err(FetchError::BadRedirect),
            "must reject redirect headers: {headers:?}"
        );
    }
}

#[test]
fn download_get_installs_with_fake_fetch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pack_bytes = make_pack("fluent", "home");
    let spec = make_spec("fluent", &pack_bytes, SAMPLE_LICENCE);
    let fetcher = FakeFetch {
        bodies: HashMap::from([
            (spec.pack_path.clone(), pack_bytes),
            (spec.licence_path.clone(), SAMPLE_LICENCE.to_vec()),
        ]),
    };
    let reg = Registry::empty();
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::Relaxed);
    };
    let env = SetEnv {
        specs: std::slice::from_ref(&spec),
        fetcher: &fetcher,
        icon_sets_dir: dir.path(),
        registry: &reg,
        reload: &reload,
    };

    let listed = handle_action(SetAction::List, &env).expect("list before install");
    match listed {
        SetNews::Listed(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, "fluent");
            assert_eq!(rows[0].name, "Fluent");
            assert!(!rows[0].installed);
        }
        other => panic!("expected Listed, got {other:?}"),
    }

    let got = handle_action(SetAction::Get("fluent".to_owned()), &env).expect("get installs pack");
    assert_eq!(got, SetNews::Installed("fluent".to_owned()));
    assert_eq!(reloads.load(Ordering::Relaxed), 1);
    assert!(reg.is_downloaded("fluent"));

    let icon_spec = IconSpec::Set {
        set: "fluent".to_owned(),
        name: "home".to_owned(),
        tone: None,
    };
    let (resolved, warning) = reg.resolve(&icon_spec, dir.path());
    assert!(warning.is_none(), "unexpected warning: {warning:?}");
    assert!(
        matches!(resolved, Resolved::Mono { .. }),
        "expected Mono icon after install, got {resolved:?}"
    );
}

#[test]
fn download_wrong_hash_installs_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pack_bytes = make_pack("fluent", "home");
    let mut spec = make_spec("fluent", &pack_bytes, SAMPLE_LICENCE);
    spec.pack_sha256[0] ^= 0xff;
    let fetcher = FakeFetch {
        bodies: HashMap::from([
            (spec.pack_path.clone(), pack_bytes),
            (spec.licence_path.clone(), SAMPLE_LICENCE.to_vec()),
        ]),
    };
    let reg = Registry::empty();
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::Relaxed);
    };
    let env = SetEnv {
        specs: std::slice::from_ref(&spec),
        fetcher: &fetcher,
        icon_sets_dir: dir.path(),
        registry: &reg,
        reload: &reload,
    };

    let err =
        handle_action(SetAction::Get("fluent".to_owned()), &env).expect_err("wrong hash must fail");
    assert!(
        err.contains("SHA-256"),
        "expected SHA-256 mismatch error, got {err:?}"
    );
    assert_eq!(reloads.load(Ordering::Relaxed), 0);
    assert!(!reg.is_downloaded("fluent"));
    let entries: Vec<_> = fs::read_dir(dir.path()).expect("read_dir").collect();
    assert!(
        entries.is_empty(),
        "nothing must be written on hash failure"
    );
}

#[test]
fn download_remove_falls_back_to_dashed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pack_bytes = make_pack("fluent", "home");
    let spec = make_spec("fluent", &pack_bytes, SAMPLE_LICENCE);
    let fetcher = FakeFetch {
        bodies: HashMap::from([
            (spec.pack_path.clone(), pack_bytes),
            (spec.licence_path.clone(), SAMPLE_LICENCE.to_vec()),
        ]),
    };
    let reg = Registry::empty();
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::Relaxed);
    };
    let env = SetEnv {
        specs: std::slice::from_ref(&spec),
        fetcher: &fetcher,
        icon_sets_dir: dir.path(),
        registry: &reg,
        reload: &reload,
    };

    handle_action(SetAction::Get("fluent".to_owned()), &env).expect("install before remove");

    let icon_spec = IconSpec::Set {
        set: "fluent".to_owned(),
        name: "home".to_owned(),
        tone: None,
    };
    assert!(matches!(
        reg.resolve(&icon_spec, dir.path()).0,
        Resolved::Mono { .. }
    ));

    let removed =
        handle_action(SetAction::Remove("fluent".to_owned()), &env).expect("remove installed set");
    assert_eq!(removed, SetNews::Removed("fluent".to_owned()));
    assert_eq!(reloads.load(Ordering::Relaxed), 2);
    assert!(!reg.is_downloaded("fluent"));

    let (resolved, _) = reg.resolve(&icon_spec, dir.path());
    assert_eq!(resolved, Resolved::Missing { dashed: true });
}

#[test]
fn download_error_messages_are_plain_language() {
    assert_eq!(
        FetchError::Network(12007).to_string(),
        "Download failed (error 12007)"
    );
    assert_eq!(
        FetchError::Status(404).to_string(),
        "The server answered 404"
    );
    assert_eq!(
        FetchError::TooLarge.to_string(),
        "The download is larger than expected"
    );
    assert_eq!(
        FetchError::BadRedirect.to_string(),
        "The download redirected to an unexpected host"
    );
    assert_eq!(
        FetchError::Unsupported.to_string(),
        "Downloads are not available on this system"
    );
}
