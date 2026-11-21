//! Build script: on a Windows target it embeds the app icon and the version info into the exe (NC46).

#![allow(
    clippy::print_stdout,
    reason = "cargo reads build script directives from stdout"
)]

use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use navo_compare_profile::brand;

/// The numeric file version `a,b,c,0` from a Cargo version such as `0.1.0` (bad parts become 0).
fn numeric_version(version: &str) -> String {
    let mut parts = version
        .split(['.', '-'])
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let mut next = || parts.next().unwrap_or(0);
    format!("{},{},{},0", next(), next(), next())
}

/// The Windows resource script: the icon (path with doubled backslashes) and one `VERSIONINFO` block.
fn resource_script(icon: &str, version: &str) -> String {
    let numeric = numeric_version(version);
    let icon = icon.replace('\\', "\\\\");
    let name = brand::NAME;
    let slug = brand::SLUG;
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
      VALUE "FileDescription", "{name}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{slug}"
      VALUE "OriginalFilename", "{slug}.exe"
      VALUE "ProductName", "{name}"
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

/// Writes the resource script and compiles it on a Windows target; does nothing elsewhere.
/// Fails when the icon file is missing (the error names its path) or the compile fails.
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest_dir
        .ancestors()
        .nth(2)
        .ok_or("crate is not inside the workspace")?;
    let icon = root.join("assets").join("brand").join("sidewise.ico");
    if !icon.is_file() {
        return Err(format!("missing {}", icon.display()).into());
    }
    println!("cargo:rerun-if-changed={}", icon.display());
    let rc = PathBuf::from(env::var("OUT_DIR")?).join(format!("{}.rc", brand::SLUG));
    let script = resource_script(&icon.display().to_string(), &env::var("CARGO_PKG_VERSION")?);
    fs::write(&rc, script)?;
    embed_resource::compile(&rc, embed_resource::NONE).manifest_required()?;
    Ok(())
}
