//! Exercises W5 discovery decisions and the bounded XML and marker parsers.

use navo_core::{FileIdentity, FileIdentityKind};
use navo_update::{Version, parse_version};
use proptest::prelude::*;

use crate::marker::Marker;
use crate::mode::{Discovery, Install, Mode, ModeError, Why, mode, parse_sq_version};

/// Parses a fixture version, failing if the test data is invalid.
fn version(text: &str) -> Version {
    parse_version(text).unwrap()
}

/// Builds a namespaced sq.version document from a package id and version.
fn xml(id: &str, version: &str) -> String {
    format!(
        "<package xmlns='urn:nuget'><metadata><id>{id}</id><version>{version}</version></metadata></package>"
    )
}

/// Builds a valid marker with a fixed 128-bit hex install id.
fn marker() -> Marker {
    Marker {
        pack_id: "NavoApp".into(),
        install_id: "0123456789abcdef0123456789abcdef".into(),
        created_by: "navo-setup 0.4.0".into(),
    }
}

/// Builds checked discovery facts for one marked installation.
fn install() -> Install {
    Install {
        root: r"C:\Apps\Navo".into(),
        identity: FileIdentity {
            volume: 1,
            file: [2; 16],
            kind: FileIdentityKind::Native,
        },
        safe_chain: true,
        update_exe: true,
        sq_version: parse_sq_version(&xml("NavoApp", "0.0.2")),
        marker: Some(Ok(marker())),
    }
}

/// Selects a mode for registry facts with no legacy installation.
fn registry_mode(found: Result<Install, Why>) -> Mode {
    mode(
        &Discovery {
            registry: Some(found),
            legacy: None,
        },
        &version("0.4.0"),
    )
}

/// No discovery source permits choosing a new folder.
#[test]
fn mode_nothing_found_is_fresh() {
    assert_eq!(mode(&Discovery::default(), &version("0.4.0")), Mode::Fresh);
}

/// Older and equal versions keep the registry installation and marker id.
#[test]
fn mode_registry_only_upgrades_or_is_same() {
    let found = install();
    assert_eq!(
        registry_mode(Ok(found.clone())),
        Mode::Upgrade(found.clone())
    );
    let discovery = Discovery {
        registry: Some(Ok(found.clone())),
        legacy: None,
    };
    assert_eq!(mode(&discovery, &version("0.0.2")), Mode::Same(found));
}

/// Both downgrade guards stop a strictly newer installed version.
#[test]
fn mode_newer_blocks_stable_and_prerelease_downgrades() {
    for (installed, payload) in [("0.0.3", "0.0.2"), ("0.4.0", "0.4.0-beta.1")] {
        let mut found = install();
        found.sq_version = parse_sq_version(&xml("NavoApp", installed));
        let discovery = Discovery {
            registry: Some(Ok(found.clone())),
            legacy: None,
        };
        assert_eq!(mode(&discovery, &version(payload)), Mode::Newer(found));
    }
}

/// A prerelease installation upgrades to the stable release.
#[test]
fn mode_prerelease_upgrades_to_stable() {
    let mut found = install();
    found.sq_version = parse_sq_version(&xml("NavoApp", "0.4.0-beta.1"));
    assert_eq!(registry_mode(Ok(found.clone())), Mode::Upgrade(found));
}

/// Build metadata does not change precedence in either direction.
#[test]
fn mode_build_metadata_is_same_in_both_directions() {
    for (installed, payload) in [("0.4.0+a", "0.4.0+z"), ("0.4.0+z", "0.4.0+a")] {
        let mut found = install();
        found.sq_version = parse_sq_version(&xml("NavoApp", installed));
        let discovery = Discovery {
            registry: Some(Ok(found.clone())),
            legacy: None,
        };
        assert_eq!(mode(&discovery, &version(payload)), Mode::Same(found));
    }
}

/// Numeric prerelease identifiers order beta.2 before beta.10.
#[test]
fn mode_prerelease_identifiers_order_numerically() {
    for (installed, payload, newer) in [
        ("0.4.0-beta.2", "0.4.0-beta.10", false),
        ("0.4.0-beta.10", "0.4.0-beta.2", true),
    ] {
        let mut found = install();
        found.sq_version = parse_sq_version(&xml("NavoApp", installed));
        let discovery = Discovery {
            registry: Some(Ok(found.clone())),
            legacy: None,
        };
        let expected = if newer {
            Mode::Newer(found)
        } else {
            Mode::Upgrade(found)
        };
        assert_eq!(mode(&discovery, &version(payload)), expected);
    }
}

/// Legacy identity permits missing markers even without a registry key.
#[test]
fn mode_legacy_only_never_becomes_fresh() {
    let mut found = install();
    found.marker = None;
    let discovery = Discovery {
        registry: None,
        legacy: Some(found.clone()),
    };
    assert_eq!(mode(&discovery, &version("0.4.0")), Mode::Upgrade(found));
}

/// Legacy identity remains valid when an existing marker cannot be parsed.
#[test]
fn mode_legacy_identity_allows_invalid_marker() {
    let mut found = install();
    found.marker = Some(Err(ModeError::Marker));
    let discovery = Discovery {
        registry: None,
        legacy: Some(found.clone()),
    };
    assert_eq!(mode(&discovery, &version("0.4.0")), Mode::Upgrade(found));
}

/// Equivalent identities merge aliases and authorize the legacy marker exception.
#[test]
fn mode_same_identity_uses_legacy_exception() {
    let mut found = install();
    found.marker = None;
    let mut legacy = found.clone();
    legacy.root = r"C:\Alias\Navo".into();
    let discovery = Discovery {
        registry: Some(Ok(found.clone())),
        legacy: Some(legacy),
    };
    assert_eq!(mode(&discovery, &version("0.4.0")), Mode::Upgrade(found));
}

/// A registry entry without a valid folder fails closed, never Fresh.
#[test]
fn mode_registry_without_folder_is_ambiguous() {
    assert_eq!(
        registry_mode(Err(Why::InvalidFolder)),
        Mode::Ambiguous(Why::InvalidFolder)
    );
    let mut found = install();
    found.safe_chain = false;
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::InvalidFolder)
    );
    let mut found = install();
    found.update_exe = false;
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::InvalidFolder)
    );
}

/// Distinct valid roots cannot share the single uninstall registry key.
#[test]
fn mode_two_valid_folders_are_ambiguous() {
    let mut legacy = install();
    legacy.identity.file = [3; 16];
    let discovery = Discovery {
        registry: Some(Ok(install())),
        legacy: Some(legacy),
    };
    assert_eq!(
        mode(&discovery, &version("0.4.0")),
        Mode::Ambiguous(Why::MultipleInstalls)
    );
}

/// A broken registry target never falls back to a different valid legacy root.
#[test]
fn mode_registry_mismatch_is_ambiguous() {
    let discovery = Discovery {
        registry: Some(Err(Why::InvalidFolder)),
        legacy: Some(install()),
    };
    assert_eq!(
        mode(&discovery, &version("0.4.0")),
        Mode::Ambiguous(Why::InvalidFolder)
    );
}

/// Damaged and foreign version documents have distinct refusal reasons.
#[test]
fn mode_damaged_and_foreign_versions_are_ambiguous() {
    let mut found = install();
    found.sq_version = parse_sq_version("broken");
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::DamagedVersion)
    );
    let mut found = install();
    found.sq_version = parse_sq_version(&xml("OtherApp", "0.0.2"));
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::ForeignVersion)
    );
}

/// A non-legacy root requires an intact Navo marker.
#[test]
fn mode_missing_and_invalid_markers_are_ambiguous() {
    let mut found = install();
    found.marker = None;
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::MissingMarker)
    );
    let mut found = install();
    found.marker = Some(Marker::parse("broken"));
    assert_eq!(
        registry_mode(Ok(found)),
        Mode::Ambiguous(Why::InvalidMarker)
    );
}

/// Unsafe uninstall characters take precedence over damaged-install repair advice.
#[test]
fn mode_unsafe_names_are_ambiguous() {
    for character in ['%', '^', '&', '!'] {
        let mut found = install();
        found.root = format!(r"C:\Apps\Navo{character}").into();
        found.safe_chain = false;
        let why = Why::UnsafeName {
            root: found.root.clone(),
            character,
        };
        assert_eq!(
            registry_mode(Ok(found.clone())),
            Mode::Ambiguous(why.clone())
        );
        let discovery = Discovery {
            registry: None,
            legacy: Some(found),
        };
        assert_eq!(mode(&discovery, &version("0.4.0")), Mode::Ambiguous(why));
    }
}

/// Unsafe names in incomplete fallback leftovers do not make them installations.
#[test]
fn mode_incomplete_unsafe_legacy_leftovers_are_ignored() {
    for character in ["", "%", "^", "&", "!"] {
        for (update_exe, readable_version) in [(false, true), (true, false), (false, false)] {
            let mut found = install();
            found.root = format!(r"C:\Apps\Navo{character}").into();
            found.update_exe = update_exe;
            if !readable_version {
                found.sq_version = Err(ModeError::Xml);
            }
            if let Some(character) = character.chars().next() {
                let why = Why::UnsafeName {
                    root: found.root.clone(),
                    character,
                };
                assert_eq!(registry_mode(Ok(found.clone())), Mode::Ambiguous(why));
            }
            let discovery = Discovery {
                registry: None,
                legacy: Some(found),
            };
            assert_eq!(
                mode(&discovery, &version("0.4.0")),
                Mode::Fresh,
                "{discovery:?}"
            );
        }
    }
}

/// Only direct package metadata fields count, regardless of XML namespace.
#[test]
fn mode_sq_version_reads_direct_namespaced_metadata() {
    let parsed = parse_sq_version(&xml("NavoApp", "0.4.0-beta.1")).unwrap();
    assert_eq!(parsed.pack_id, "NavoApp");
    assert_eq!(parsed.version, version("0.4.0-beta.1"));
    assert!(
        parse_sq_version(
            "<package><other><id>NavoApp</id><version>0.4.0</version></other></package>"
        )
        .is_err()
    );
    assert!(parse_sq_version("<outer><package><metadata><id>NavoApp</id><version>0.4.0</version></metadata></package></outer>").is_err());
}

/// Missing, duplicate, malformed and entity-bearing documents are refused.
#[test]
fn mode_sq_version_rejects_damaged_input() {
    for input in [
        "",
        "<package>",
        "<package><metadata><id>NavoApp</id></metadata></package>",
        "<package><metadata><id>NavoApp</id><version>bad</version></metadata></package>",
        "<package><metadata><id>NavoApp</id><id>NavoApp</id><version>0.4.0</version></metadata></package>",
        "<package><metadata><id><nested>NavoApp</nested></id><version>0.4.0</version></metadata></package>",
        "<!DOCTYPE package [<!ENTITY id 'NavoApp'>]><package><metadata><id>&id;</id><version>0.4.0</version></metadata></package>",
    ] {
        assert!(parse_sq_version(input).is_err(), "accepted {input}");
    }
}

/// Multibyte XML is accepted through 64 KiB, then refused one byte over.
#[test]
fn mode_sq_version_enforces_byte_limit() {
    let base = xml("NavoApp", "0.4.0").replace("</package>", "<!--");
    for size in [65535, 65536, 65537] {
        let remaining = size - base.len() - "--></package>".len();
        let padded = format!(
            "{base}{}{}--></package>",
            "é".repeat(remaining / 2),
            " ".repeat(remaining % 2)
        );
        assert_eq!(padded.len(), size);
        let expected = if size <= 65536 {
            Ok(crate::mode::SqVersion {
                pack_id: "NavoApp".into(),
                version: version("0.4.0"),
            })
        } else {
            Err(ModeError::TooLarge)
        };
        assert_eq!(parse_sq_version(&padded), expected);
    }
}

proptest! {
    /// Arbitrary Unicode input never panics in either parser.
    #[test]
    fn mode_parsers_never_panic(input in any::<String>()) {
        let _ = parse_sq_version(&input);
        let _ = Marker::parse(&input);
    }

    /// Structured semver XML retains the exact numeric version.
    #[test]
    fn mode_sq_version_preserves_semver(major in any::<u16>(), minor in any::<u16>(), patch in any::<u16>()) {
        let text = format!("{major}.{minor}.{patch}");
        let parsed = parse_sq_version(&xml("NavoApp", &text)).unwrap();
        prop_assert_eq!(parsed.version, version(&text));
    }
}
