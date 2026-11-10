//! Embeds optional installer payloads and the Windows setup resources.

#![allow(clippy::print_stdout, reason = "Cargo build directives use stdout")]

use sha2::{Digest, Sha256};
use std::{env, error::Error, ffi::OsString, fs, io, io::Read, path::Path};

include!("../navo-app/build_resource.rs");

/// Copies configured payloads and builds Windows resources; invalid release inputs fail.
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../navo-app/build_resource.rs");
    for name in [
        "NAVO_SETUP_PAYLOAD",
        "NAVO_SETUP_PACKAGE",
        "NAVO_SETUP_REQUIRE_PAYLOAD",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let out = std::path::PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let inputs = payload_inputs();
    let mut source = "pub const AVAILABLE: bool = false;\n".to_owned();
    if let Some((setup, package)) = inputs {
        let package = Path::new(&package);
        let name = package
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid package name")?;
        source = "pub const AVAILABLE: bool = true;\n".to_owned();
        source.push_str(&copy_payload(Path::new(&setup), &out, "SETUP")?);
        source.push_str(&copy_payload(package, &out, "PACKAGE")?);
        source.push_str(&format!("pub const PACKAGE_NAME: &str = {name:?};\n"));
    } else {
        if env::var("NAVO_SETUP_REQUIRE_PAYLOAD").as_deref() == Ok("1") {
            return Err(
                "NAVO_SETUP_PAYLOAD and a -full.nupkg NAVO_SETUP_PACKAGE are required".into(),
            );
        }
        for label in ["SETUP", "PACKAGE"] {
            source.push_str(&format!("pub const {label}: &[u8] = &[];\npub const {label}_SHA256: [u8; 32] = [0; 32];\npub const {label}_SIZE: u64 = 0;\n"));
        }
        source.push_str("pub const PACKAGE_NAME: &str = \"\";\n");
    }
    fs::write(
        out.join("payload.rs"),
        format!("// Generated payload bytes and build metadata.\n{source}"),
    )?;
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        compile_resources(&out)?;
        link_dll_guard();
    }
    Ok(())
}

/// Reads paired payload paths; warns and omits incomplete inputs or non-full package names.
fn payload_inputs() -> Option<(OsString, OsString)> {
    let setup = env::var_os("NAVO_SETUP_PAYLOAD");
    let package = env::var_os("NAVO_SETUP_PACKAGE");
    match (setup, package) {
        (Some(setup), Some(package))
            if Path::new(&package)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-full.nupkg")) =>
        {
            Some((setup, package))
        }
        (None, None) => None,
        _ => {
            println!(
                "cargo:warning=Both payload variables must be set and NAVO_SETUP_PACKAGE must name a -full.nupkg file"
            );
            None
        }
    }
}

/// Copies one payload, hashes the copied bytes, and emits its Rust constants.
fn copy_payload(input: &Path, out: &Path, label: &str) -> Result<String, Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", input.display());
    let destination = out.join(label);
    fs::copy(input, &destination)?;
    let mut hash = Sha256::new();
    let mut file = fs::File::open(&destination)?;
    let mut buffer = [0; 8192];
    let mut size = 0_u64;
    loop {
        let count = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    let digest: [u8; 32] = hash.finalize().into();
    Ok(format!(
        "pub const {label}: &[u8] = include_bytes!({destination:?});\npub const {label}_SHA256: [u8; 32] = {digest:?};\npub const {label}_SIZE: u64 = {size};\n"
    ))
}

/// Imports outside KnownDLLs, from `dumpbin /imports` on a build with a payload (IW24).
///
/// Without a payload the linker drops the runner, so check a payload build.
/// `scripts/setup-imports.ps1` fails when a new one appears without being listed here.
const DELAY_LOAD: &[&str] = &["bcrypt.dll", "bcryptprimitives.dll"];

/// Links Setup so its imports load from System32 only (IW24 DLL planting defence).
///
/// `0x800` is `LOAD_LIBRARY_SEARCH_SYSTEM32` for the image's own load-time dependencies.
fn link_dll_guard() {
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    if !DELAY_LOAD.is_empty() {
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    for dll in DELAY_LOAD {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
    }
}

/// Embeds the app's icon and version resource. The asInvoker/PerMonitorV2 manifest
/// (IW13) comes from gpui-pre's default `windows-manifest` feature; a second one fails the link.
fn compile_resources(out: &Path) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let icon = root.join("assets/brand/navo.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let version = env::var("CARGO_PKG_VERSION")?;
    let rc = resource_script(Resource {
        icon: &icon.display().to_string(),
        version: &version,
        product_version: &version,
        description: "Navo Setup",
        internal_name: "navo-setup",
        original_filename: "Navo-Setup.exe",
    });
    let resource = out.join("setup.rc");
    fs::write(&resource, rc)?;
    embed_resource::compile(&resource, embed_resource::NONE).manifest_required()?;
    Ok(())
}
