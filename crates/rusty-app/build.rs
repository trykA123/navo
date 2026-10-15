#![allow(
    clippy::print_stdout,
    reason = "cargo reads build script directives from stdout"
)]

use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    emit_commit();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest_dir
        .ancestors()
        .nth(2)
        .ok_or("crate is not inside the workspace")?;
    let icon = root.join("assets").join("brand").join("ferrite.ico");
    if !icon.is_file() {
        return Err(format!("missing {}", icon.display()).into());
    }
    println!("cargo:rerun-if-changed={}", icon.display());

    let version = env::var("CARGO_PKG_VERSION")?;
    let rc = PathBuf::from(env::var("OUT_DIR")?).join("ferrite.rc");
    fs::write(&rc, resource_script(&icon.display().to_string(), &version))?;
    embed_resource::compile(&rc, embed_resource::NONE).manifest_required()?;
    Ok(())
}

fn emit_commit() {
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=FERRITE_COMMIT={commit}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| output.status.success() && !text.is_empty())
}

fn resource_script(icon: &str, version: &str) -> String {
    let mut parts = version
        .split(['.', '-'])
        .map(|p| p.parse::<u16>().unwrap_or(0));
    let mut next = || parts.next().unwrap_or(0);
    let numeric = format!("{},{},{},0", next(), next(), next());
    let icon = icon.replace('\\', "\\\\");
    format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Ferrite"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "ferrite"
      VALUE "OriginalFilename", "ferrite.exe"
      VALUE "ProductName", "Ferrite"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
