#![allow(
    clippy::print_stdout,
    reason = "cargo reads build script directives from stdout"
)]

use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

include!("build_resource.rs");

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_resource.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    println!("cargo:rerun-if-env-changed=NAVO_BUILD");
    emit_commit();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest_dir
        .ancestors()
        .nth(2)
        .ok_or("crate is not inside the workspace")?;
    let icon = root.join("assets").join("brand").join("navo.ico");
    if !icon.is_file() {
        return Err(format!("missing {}", icon.display()).into());
    }
    println!("cargo:rerun-if-changed={}", icon.display());

    let version = env::var("CARGO_PKG_VERSION")?;
    let product_version = product_version(&version);
    let rc = PathBuf::from(env::var("OUT_DIR")?).join("navo.rc");
    fs::write(
        &rc,
        resource_script(Resource {
            icon: &icon.display().to_string(),
            version: &version,
            product_version: &product_version,
            description: "Navo",
            internal_name: "navo",
            original_filename: "navo.exe",
        }),
    )?;
    embed_resource::compile(&rc, embed_resource::NONE).manifest_required()?;
    Ok(())
}

fn product_version(version: &str) -> String {
    match env::var("NAVO_BUILD")
        .ok()
        .filter(|build| !build.is_empty())
    {
        Some(build) => format!("{version} ({build})"),
        None => version.to_owned(),
    }
}

fn emit_commit() {
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=NAVO_COMMIT={commit}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| output.status.success() && !text.is_empty())
}
