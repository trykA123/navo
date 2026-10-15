pub(super) const TEXT: &str = "text";

pub(super) const EXTENSIONS: [(&str, &[&str]); 47] = [
    ("astro", &["astro"]),
    ("asm", &["s", "asm"]),
    ("bash", &["sh", "zsh", "bash"]),
    ("batch", &["bat", "cmd"]),
    ("c", &["c", "h"]),
    ("cmake", &["cmake"]),
    ("cpp", &["cpp", "hpp", "hh", "cc", "cxx"]),
    ("csharp", &["cs"]),
    ("css", &["css"]),
    ("diff", &["diff"]),
    ("dockerfile", &["dockerfile", "containerfile"]),
    ("ejs", &["ejs"]),
    ("elixir", &["ex", "exs"]),
    ("erb", &["erb"]),
    ("go", &["go"]),
    ("graphql", &["graphql"]),
    ("html", &["html", "htm", "xhtml"]),
    ("ini", &["ini", "inf"]),
    ("java", &["java"]),
    ("javascript", &["js", "jsx", "mjs", "cjs"]),
    ("json", &["json", "jsonc"]),
    ("json5", &["json5"]),
    ("kotlin", &["kt", "kts"]),
    ("latex", &["tex", "sty", "ltx"]),
    ("lua", &["lua"]),
    ("make", &["mk"]),
    ("markdown", &["md", "mdx"]),
    ("objc", &["mm"]),
    ("perl", &["pl", "pm"]),
    ("php", &["php"]),
    ("powershell", &["ps1", "psm1", "psd1"]),
    ("properties", &["properties"]),
    ("proto", &["proto"]),
    ("python", &["py", "pyi"]),
    ("r", &["r"]),
    ("ruby", &["rb"]),
    ("rust", &["rs"]),
    ("scala", &["scala"]),
    ("sql", &["sql"]),
    ("svelte", &["svelte"]),
    ("swift", &["swift"]),
    ("toml", &["toml"]),
    ("tsx", &["tsx"]),
    ("typescript", &["ts", "mts", "cts"]),
    ("xml", XML),
    ("yaml", &["yml", "yaml"]),
    ("zig", &["zig"]),
];

const XML: &[&str] = &[
    "xml", "xaml", "svg", "plist", "csproj", "vcxproj", "vbproj", "fsproj", "props", "targets",
    "resx", "nuspec", "xsd", "xsl", "xslt", "config", "arxml",
];

pub(super) const SPECIAL: [(&str, &str); 3] = [
    ("makefile", "make"),
    ("gnumakefile", "make"),
    ("cmakelists.txt", "cmake"),
];

pub(super) const ALIASES: [(&str, &str); 14] = [
    ("shell", "bash"),
    ("cperl", "perl"),
    ("c++", "cpp"),
    ("dosini", "ini"),
    ("conf", "properties"),
    ("jproperties", "properties"),
    ("nxml", "xml"),
    ("plaintex", "latex"),
    ("objective-c", "objc"),
    ("objectivec", "objc"),
    ("nasm", "asm"),
    ("docker", "dockerfile"),
    ("makefile", "make"),
    ("pwsh", "powershell"),
];

pub(super) fn special(lower: &str) -> Option<&'static str> {
    let stem = lower.split_once('.').map_or(lower, |(stem, _)| stem);
    if matches!(stem, "dockerfile" | "containerfile") {
        return Some("dockerfile");
    }
    SPECIAL
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, language)| *language)
}

pub(super) fn by_extension(ext: &str) -> Option<&'static str> {
    EXTENSIONS
        .iter()
        .find(|(name, exts)| exts.contains(&ext) || *name == ext)
        .map(|(name, _)| *name)
}

pub(super) fn resolve(token: &str) -> Option<&'static str> {
    let token = token.to_ascii_lowercase();
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == token)
        .map(|(_, language)| *language)
        .or_else(|| by_extension(&token))
}

#[cfg(test)]
pub(super) fn all() -> impl Iterator<Item = &'static str> {
    EXTENSIONS.iter().map(|(name, _)| *name)
}
