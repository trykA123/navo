//! File-type icons: the prototype's `ICON_MAP` (Material Icon Theme 5.40.0), variants in pack order.

pub mod cache;
#[cfg(test)]
mod cache_tests;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MaterialIcon {
    C,
    Console,
    Css,
    Database,
    Dll,
    Docker,
    Document,
    Exe,
    File,
    Folder,
    FolderConfig,
    FolderConfigOpen,
    FolderDocs,
    FolderDocsOpen,
    FolderGithub,
    FolderGithubOpen,
    FolderLib,
    FolderLibOpen,
    FolderOpen,
    FolderResource,
    FolderResourceOpen,
    FolderScripts,
    FolderScriptsOpen,
    FolderSrc,
    FolderSrcOpen,
    FolderTest,
    FolderTestOpen,
    Freemarker,
    Git,
    Go,
    H,
    Hex,
    Html,
    Image,
    Javascript,
    Json,
    License,
    Lock,
    Log,
    Markdown,
    Powershell,
    Python,
    ReactTs,
    Readme,
    Rust,
    Svg,
    Toml,
    Typescript,
    Yaml,
    Zip,
}

/// The mark of a file or folder name; `expanded` swaps a folder to its open mark.
pub(crate) fn icon_id(name: &str, is_dir: bool, expanded: bool) -> MaterialIcon {
    let lower = name.to_lowercase();
    if is_dir {
        let (closed, open) = match lower.as_str() {
            "src" => (MaterialIcon::FolderSrc, MaterialIcon::FolderSrcOpen),
            "test" | "tests" => (MaterialIcon::FolderTest, MaterialIcon::FolderTestOpen),
            "docs" => (MaterialIcon::FolderDocs, MaterialIcon::FolderDocsOpen),
            "assets" => (
                MaterialIcon::FolderResource,
                MaterialIcon::FolderResourceOpen,
            ),
            ".github" => (MaterialIcon::FolderGithub, MaterialIcon::FolderGithubOpen),
            "crates" => (MaterialIcon::FolderLib, MaterialIcon::FolderLibOpen),
            "scripts" => (MaterialIcon::FolderScripts, MaterialIcon::FolderScriptsOpen),
            "config" => (MaterialIcon::FolderConfig, MaterialIcon::FolderConfigOpen),
            _ => (MaterialIcon::Folder, MaterialIcon::FolderOpen),
        };
        return if expanded { open } else { closed };
    }
    match lower.as_str() {
        "cargo.toml" | "rust-toolchain.toml" | "clippy.toml" | "deny.toml" => MaterialIcon::Rust,
        "cargo.lock" => MaterialIcon::Lock,
        "dockerfile" => MaterialIcon::Docker,
        ".gitignore" | ".gitattributes" => MaterialIcon::Git,
        "license" | "license.md" => MaterialIcon::License,
        "readme" | "readme.md" => MaterialIcon::Readme,
        "taplo.toml" | "typos.toml" | "size-exceptions.toml" => MaterialIcon::Toml,
        _ => ext_icon(&lower),
    }
}

/// The mark of the name's extension; a leading dot is not one, so dotfiles keep the file mark.
fn ext_icon(name: &str) -> MaterialIcon {
    let dot = name.rfind('.').unwrap_or(0);
    let ext = if dot > 0 { &name[dot + 1..] } else { "" };
    match ext {
        "rs" => MaterialIcon::Rust,
        "toml" => MaterialIcon::Toml,
        "ts" => MaterialIcon::Typescript,
        "tsx" => MaterialIcon::ReactTs,
        "js" => MaterialIcon::Javascript,
        "json" => MaterialIcon::Json,
        "md" => MaterialIcon::Markdown,
        "yml" | "yaml" => MaterialIcon::Yaml,
        "html" => MaterialIcon::Html,
        "css" => MaterialIcon::Css,
        "svg" => MaterialIcon::Svg,
        "png" | "jpg" | "jpeg" | "ico" => MaterialIcon::Image,
        "lock" => MaterialIcon::Lock,
        "log" => MaterialIcon::Log,
        "txt" => MaterialIcon::Document,
        "sh" => MaterialIcon::Console,
        "ps1" => MaterialIcon::Powershell,
        "py" => MaterialIcon::Python,
        "go" => MaterialIcon::Go,
        "c" => MaterialIcon::C,
        "h" => MaterialIcon::H,
        "sql" => MaterialIcon::Database,
        "zip" | "7z" => MaterialIcon::Zip,
        "exe" => MaterialIcon::Exe,
        "dll" => MaterialIcon::Dll,
        "bin" | "hex" | "dat" => MaterialIcon::Hex,
        "ftl" => MaterialIcon::Freemarker,
        _ => MaterialIcon::File,
    }
}

#[cfg(test)]
mod tests;
