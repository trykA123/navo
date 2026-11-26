//! Packs and verifies `NVIC1` icon packs from pinned npm tarballs (BB-D6, BB-D8, BB-D12).

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use navo_icons::pack::{self, Pack, SetDraft};
use navo_icons::sources::{self, Reader, SourceSpec, Style};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

const DEFAULT_CONFIG: &str = "assets/icons/sources.toml";
const DEFAULT_OUT: &str = "assets/icons/bundled.nvic";
const DEFAULT_LICENCES: &str = "assets/icons/licences";

/// Parsed contents of `assets/icons/sources.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcesConfig {
    /// Packages built into `assets/icons/bundled.nvic`.
    #[serde(default)]
    pub(crate) bundled: Vec<SourcePackage>,
    /// Optional single-set packs verified with `--downloads`.
    #[serde(default)]
    pub(crate) download: Vec<SourcePackage>,
}

/// One upstream npm tarball and the icon sets and licence files taken from it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePackage {
    /// Upstream npm package name.
    pub(crate) package: String,
    /// Pinned upstream version string.
    pub(crate) version: String,
    /// Canonical tarball URL (filename after the last `/` is read from `--tarballs`).
    pub(crate) url: String,
    /// Expected lowercase SHA-256 hex digest of the tarball.
    pub(crate) sha256: String,
    /// SPDX licence identifier written into set records.
    pub(crate) licence: String,
    /// Licence or disclaimer files copied into the licences directory.
    #[serde(default)]
    pub(crate) licences: Vec<LicenceFile>,
    /// One or more icon sets produced from this tarball.
    pub(crate) sets: Vec<SetConfig>,
}

/// Maps one licence file path inside a tarball to its output filename.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LicenceFile {
    /// Path inside the npm tarball.
    pub(crate) from: String,
    /// Output filename inside the licences directory.
    pub(crate) to: String,
}

/// One icon set definition inside a `SourcePackage`.
#[derive(Debug, Deserialize)]
pub(crate) struct SetConfig {
    /// Set identifier written into the pack.
    pub(crate) id: String,
    /// Icon grid size in pixels.
    pub(crate) grid: u16,
    /// Reader layout inside the tarball.
    #[serde(flatten)]
    pub(crate) reader: ReaderConfig,
}

/// Reader configuration matching `navo_icons::sources::Reader`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum ReaderConfig {
    /// Lucide and Tabler node JSON files.
    Nodes {
        /// Path to the node map JSON inside the tarball.
        file: String,
        /// Drawing style (`stroke` or `fill`).
        style: StyleConfig,
        /// Optional path to a tags JSON file inside the tarball.
        #[serde(default)]
        tags: Option<String>,
    },
    /// Directory of individual SVG files.
    SvgDir {
        /// Directory prefix inside the tarball.
        dir: String,
        /// Filename suffix to include.
        suffix: String,
        /// Name transform (`""` or `"underscore"`).
        #[serde(default)]
        strip: String,
        /// Optional filename suffix to exclude.
        #[serde(default)]
        skip_suffix: Option<String>,
    },
    /// Simple Icons layout with strict licence filtering.
    SimpleIcons,
}

/// Drawing style for `ReaderConfig::Nodes`.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StyleConfig {
    /// Outline stroke icons.
    Stroke,
    /// Solid fill icons.
    Fill,
}

struct PackArgs {
    tarballs: PathBuf,
    config: PathBuf,
    out: PathBuf,
    licences: PathBuf,
}

struct VerifyArgs {
    tarballs: PathBuf,
    downloads: bool,
    config: PathBuf,
    pack: PathBuf,
}

struct VerifiedPackage {
    tarball_len: usize,
    sha256: String,
    sets: Vec<SetDraft>,
    licences: Vec<(String, Vec<u8>)>,
}

struct BundledBuild {
    sets: Vec<SetDraft>,
    licences: Vec<(String, Vec<u8>)>,
}

/// CLI entry point for `cargo xtask icons`.
pub fn run(args: &[String]) -> ExitCode {
    match execute(args) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask icons: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Executes `icons` CLI arguments and returns output lines on success or an error message.
pub(crate) fn execute(args: &[String]) -> Result<Vec<String>, String> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("pack") => run_pack(&parse_pack_args(rest)?),
        Some("verify") => run_verify(&parse_verify_args(rest)?),
        _ => Err(
            "usage: cargo xtask icons <pack --tarballs <dir> [--config <sources.toml>] [--out <bundled.nvic>] [--licences <dir>] | verify --tarballs <dir> [--downloads] [--config <sources.toml>] [--pack <bundled.nvic>]>"
                .to_owned(),
        ),
    }
}

/// Parses and validates a `sources.toml` file from disk.
pub(crate) fn load_config(path: &Path) -> Result<SourcesConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let config: SourcesConfig =
        toml::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))?;
    for pkg in config.bundled.iter().chain(&config.download) {
        validate_package(pkg)?;
    }
    Ok(config)
}

/// Checks required invariants on one `SourcePackage` entry.
fn validate_package(pkg: &SourcePackage) -> Result<(), String> {
    if pkg.sha256.len() != 64 || !pkg.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "invalid sha256 for `{}`: expected 64 hex chars",
            pkg.package
        ));
    }
    tarball_filename(&pkg.url)?;
    if pkg.sets.is_empty() {
        return Err(format!("package `{}` declares no icon sets", pkg.package));
    }
    for licence in &pkg.licences {
        validate_licence_filename(&licence.to)?;
    }
    Ok(())
}

/// Ensures a licence output name is a single normal filename with no path separators or traversal.
fn validate_licence_filename(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(format!(
            "invalid licence filename `{name}`: must be a single filename without separators"
        ));
    }
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!(
            "invalid licence filename `{name}`: must be a single normal path component"
        )),
    }
}

/// Parses arguments for `cargo xtask icons pack`.
fn parse_pack_args(args: &[String]) -> Result<PackArgs, String> {
    let mut tarballs: Option<PathBuf> = None;
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut licences = PathBuf::from(DEFAULT_LICENCES);
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let next = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--tarballs" => tarballs = Some(PathBuf::from(next)),
            "--config" => config = PathBuf::from(next),
            "--out" => out = PathBuf::from(next),
            "--licences" => licences = PathBuf::from(next),
            other => return Err(format!("unknown flag for pack: {other}")),
        }
        index += 2;
    }
    let tarballs = tarballs.ok_or_else(|| "missing required --tarballs <dir>".to_owned())?;
    Ok(PackArgs {
        tarballs,
        config,
        out,
        licences,
    })
}

/// Parses arguments for `cargo xtask icons verify`.
fn parse_verify_args(args: &[String]) -> Result<VerifyArgs, String> {
    let mut tarballs: Option<PathBuf> = None;
    let mut downloads = false;
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut pack = PathBuf::from(DEFAULT_OUT);
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--downloads" {
            downloads = true;
            index += 1;
            continue;
        }
        let next = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--tarballs" => tarballs = Some(PathBuf::from(next)),
            "--config" => config = PathBuf::from(next),
            "--pack" => pack = PathBuf::from(next),
            other => return Err(format!("unknown flag for verify: {other}")),
        }
        index += 2;
    }
    let tarballs = tarballs.ok_or_else(|| "missing required --tarballs <dir>".to_owned())?;
    Ok(VerifyArgs {
        tarballs,
        downloads,
        config,
        pack,
    })
}

/// Builds `bundled.nvic`, copies licence files, and returns one summary line per set.
fn run_pack(opts: &PackArgs) -> Result<Vec<String>, String> {
    let config = load_config(&opts.config)?;
    let built = build_bundled(&config.bundled, &opts.tarballs)?;
    let bytes =
        pack::write(&built.sets).map_err(|error| format!("cannot write icon pack: {error}"))?;
    if let Some(parent) = opts.out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    std::fs::write(&opts.out, &bytes)
        .map_err(|error| format!("cannot write {}: {error}", opts.out.display()))?;
    write_licences(&opts.licences, &built.licences)?;
    let lines = built
        .sets
        .iter()
        .map(|set| {
            let raw_bytes: usize = set.icons.iter().map(|icon| icon.svg.len()).sum();
            format!("{}: {} icons ({raw_bytes} bytes)", set.id, set.icons.len())
        })
        .collect();
    Ok(lines)
}

/// Rebuilds and verifies either `bundled.nvic` or all `[[download]]` single-set packs.
fn run_verify(opts: &VerifyArgs) -> Result<Vec<String>, String> {
    let config = load_config(&opts.config)?;
    if opts.downloads {
        return verify_downloads(&config.download, &opts.tarballs);
    }
    let built = build_bundled(&config.bundled, &opts.tarballs)?;
    let rebuilt =
        pack::write(&built.sets).map_err(|error| format!("cannot build icon pack: {error}"))?;
    let existing = std::fs::read(&opts.pack)
        .map_err(|error| format!("cannot read {}: {error}", opts.pack.display()))?;
    if existing == rebuilt {
        return Ok(vec!["bundled.nvic matches".to_owned()]);
    }
    Err(describe_mismatch(&existing, &rebuilt))
}

/// Verifies every `[[download]]` tarball and builds its single-set pack in memory.
fn verify_downloads(packages: &[SourcePackage], tarballs: &Path) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for pkg in packages {
        let verified = read_verified_package(pkg, tarballs)?;
        for set in verified.sets {
            let packed = pack::write(std::slice::from_ref(&set))
                .map_err(|error| format!("cannot pack `{}`: {error}", set.id))?;
            lines.push(format!(
                "{} matches {} {} -> {}",
                set.id,
                verified.sha256,
                verified.tarball_len,
                packed.len()
            ));
        }
    }
    Ok(lines)
}

/// Reads and builds all `[[bundled]]` packages into sets and licence pairs.
fn build_bundled(packages: &[SourcePackage], tarballs: &Path) -> Result<BundledBuild, String> {
    let mut sets = Vec::new();
    let mut licences = Vec::new();
    for pkg in packages {
        let verified = read_verified_package(pkg, tarballs)?;
        sets.extend(verified.sets);
        licences.extend(verified.licences);
    }
    Ok(BundledBuild { sets, licences })
}

/// Writes extracted licence files into `dir`.
fn write_licences(dir: &Path, licences: &[(String, Vec<u8>)]) -> Result<(), String> {
    if licences.is_empty() {
        return Ok(());
    }
    for (name, _) in licences {
        validate_licence_filename(name)?;
    }
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    for (name, bytes) in licences {
        let path = dir.join(name);
        std::fs::write(&path, bytes)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}

/// Reads one tarball from disk, verifies its SHA-256, extracts licences, and builds its sets.
fn read_verified_package(pkg: &SourcePackage, tarballs: &Path) -> Result<VerifiedPackage, String> {
    let filename = tarball_filename(&pkg.url)?;
    let path = tarballs.join(filename);
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("cannot read tarball {}: {error}", path.display()))?;
    let actual = sha256_hex(&bytes);
    let expected = pkg.sha256.to_ascii_lowercase();
    if actual != expected {
        return Err(format!(
            "sha256 mismatch for {} ({filename}): expected {expected}, got {actual}",
            pkg.package
        ));
    }
    let files = sources::read_tarball(&bytes, &|entry| is_wanted(pkg, entry))
        .map_err(|error| format!("cannot read tarball {}: {error}", path.display()))?;
    let mut licences = Vec::with_capacity(pkg.licences.len());
    for licence in &pkg.licences {
        let content = files.get(&licence.from).ok_or_else(|| {
            format!(
                "missing licence file `{}` in {} ({filename})",
                licence.from, pkg.package
            )
        })?;
        licences.push((licence.to.clone(), content.clone()));
    }
    let mut sets = Vec::with_capacity(pkg.sets.len());
    for set in &pkg.sets {
        let spec = to_source_spec(pkg, set);
        let draft = sources::build_set(&files, &spec)
            .map_err(|error| format!("cannot build set `{}`: {error}", set.id))?;
        sets.push(draft);
    }
    Ok(VerifiedPackage {
        tarball_len: bytes.len(),
        sha256: actual,
        sets,
        licences,
    })
}

/// Converts a config set entry into `navo_icons::sources::SourceSpec`.
fn to_source_spec(pkg: &SourcePackage, set: &SetConfig) -> SourceSpec {
    let reader = match &set.reader {
        ReaderConfig::Nodes { file, style, tags } => Reader::Nodes {
            file: file.clone(),
            style: match style {
                StyleConfig::Stroke => Style::Stroke,
                StyleConfig::Fill => Style::Fill,
            },
            tags: tags.clone(),
        },
        ReaderConfig::SvgDir {
            dir,
            suffix,
            strip,
            skip_suffix,
        } => Reader::SvgDir {
            dir: dir.clone(),
            suffix: suffix.clone(),
            strip: strip.clone(),
            skip_suffix: skip_suffix.clone(),
        },
        ReaderConfig::SimpleIcons => Reader::SimpleIcons,
    };
    SourceSpec {
        id: set.id.clone(),
        version: pkg.version.clone(),
        licence: pkg.licence.clone(),
        grid: set.grid,
        reader,
    }
}

/// Reports whether a tarball entry path is needed for any set or licence in `pkg`.
fn is_wanted(pkg: &SourcePackage, path: &str) -> bool {
    if pkg.licences.iter().any(|licence| licence.from == path) {
        return true;
    }
    pkg.sets.iter().any(|set| match &set.reader {
        ReaderConfig::Nodes { file, tags, .. } => path == file || tags.as_deref() == Some(path),
        ReaderConfig::SvgDir {
            dir,
            suffix,
            skip_suffix,
            ..
        } => {
            let Some(rest) = under_dir(path, dir) else {
                return false;
            };
            rest.ends_with(suffix)
                && !skip_suffix
                    .as_deref()
                    .is_some_and(|skip| rest.ends_with(skip))
        }
        ReaderConfig::SimpleIcons => {
            path == "package/data/simple-icons.json"
                || path == "data/simple-icons.json"
                || ((path.starts_with("package/icons/") || path.starts_with("icons/"))
                    && path.ends_with(".svg"))
        }
    })
}

/// Returns the relative path below `dir`, matching `navo_icons::sources::svg_dir`.
fn under_dir<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

/// Extracts the filename component after the last `/` in a tarball URL.
fn tarball_filename(url: &str) -> Result<&str, String> {
    let name = url.rsplit('/').next().unwrap_or_default();
    if name.is_empty() {
        return Err(format!("invalid tarball url `{url}`: missing filename"));
    }
    Ok(name)
}

/// Computes the lowercase 64-character SHA-256 hex digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Names the first differing set (when both packs parse) and first differing byte offset.
fn describe_mismatch(existing: &[u8], rebuilt: &[u8]) -> String {
    let first_byte = existing
        .iter()
        .zip(rebuilt)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| existing.len().min(rebuilt.len()));
    let set_note = first_differing_set(existing, rebuilt).map_or_else(String::new, |name| {
        format!(" (first differing set: {name})")
    });
    format!(
        "bundled.nvic mismatch{set_note}: first differing byte at offset {first_byte} (existing {} bytes, rebuilt {} bytes)",
        existing.len(),
        rebuilt.len()
    )
}

/// Inspects two packs and returns the id of the first set whose metadata, names, tags, or bodies differ.
fn first_differing_set(existing: &[u8], rebuilt: &[u8]) -> Option<String> {
    let left = Pack::open(existing).ok()?;
    let right = Pack::open(rebuilt).ok()?;
    for (l_info, r_info) in left.sets().iter().zip(right.sets()) {
        if l_info != r_info || !set_contents_equal(&left, &right, &l_info.id) {
            return Some(l_info.id.clone());
        }
    }
    if left.sets().len() != right.sets().len() {
        let extra = left
            .sets()
            .get(right.sets().len())
            .or_else(|| right.sets().get(left.sets().len()))?;
        return Some(extra.id.clone());
    }
    None
}

/// Compares names, tags, and decoded bodies of one set across two opened packs.
fn set_contents_equal(left: &Pack<'_>, right: &Pack<'_>, id: &str) -> bool {
    let left_names = left.names(id);
    if left_names != right.names(id) || left.tags(id).ok() != right.tags(id).ok() {
        return false;
    }
    for name in left_names {
        let (Some(l_ref), Some(r_ref)) = (left.find(id, name), right.find(id, name)) else {
            return false;
        };
        if l_ref.brand != r_ref.brand {
            return false;
        }
        let (Ok(l_chunk), Ok(r_chunk)) =
            (left.chunk(id, l_ref.chunk), right.chunk(id, r_ref.chunk))
        else {
            return false;
        };
        if pack::body(&l_chunk, &l_ref) != pack::body(&r_chunk, &r_ref) {
            return false;
        }
    }
    true
}
