//! Icon registry over the embedded bundled pack, downloaded packs, and stored user icon files.

mod files;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};

use crate::pack::{self, IconRef, Pack, PackError, SetInfo};
use crate::spec::{BrandColour, IconSpec, Shape, Tone};

/// Embedded `NVIC1` pack holding the bundled icon sets.
pub static BUNDLED: &[u8] = include_bytes!("../../../assets/icons/bundled.nvic");

/// Combined upstream licence notice for the bundled icon sets.
pub static NOTICE_TXT: &str = include_str!("../../../assets/icons/licences/NOTICE.txt");

/// Maximum number of decoded pack chunks kept in the LRU cache.
const MAX_CACHED_CHUNKS: usize = 8;
/// Known optional downloadable set ids (BB-D12).
const DOWNLOAD_SET_IDS: &[&str] = &["fluent", "fluent-filled", "material", "carbon"];

/// Resolved icon ready for the UI layer to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// Single-colour SVG tinted by theme tone or brand colour.
    Mono {
        /// Normalised SVG bytes.
        svg: Arc<[u8]>,
        /// Optional theme tone override.
        tone: Option<Tone>,
        /// Optional brand RGB colour.
        brand: Option<[u8; 3]>,
    },
    /// Multi-colour SVG drawn with its own colours.
    Colour {
        /// Normalised SVG bytes.
        svg: Arc<[u8]>,
    },
    /// Re-encoded RGBA8 PNG raster icon.
    Raster {
        /// Validated PNG bytes.
        png: Arc<[u8]>,
    },
    /// One or two upper-cased letters on a background shape.
    Letters {
        /// The upper-cased letters.
        letters: String,
        /// The background shape.
        shape: Shape,
        /// The letter colour tone.
        tone: Tone,
    },
    /// A Lucide glyph with an optional status dot.
    Glyph {
        /// Normalised Lucide SVG bytes.
        svg: Arc<[u8]>,
        /// The glyph colour tone.
        tone: Tone,
        /// Optional status dot colour tone.
        dot: Option<Tone>,
    },
    /// Icon could not be resolved; `dashed` marks an uninstalled optional download set.
    Missing {
        /// True when the icon belongs to a known optional download set that is not installed.
        dashed: bool,
    },
}

struct DownloadedPack {
    sets: Vec<SetInfo>,
    bytes: Arc<[u8]>,
}

struct CachedChunk {
    set: String,
    chunk: u16,
    data: Arc<[u8]>,
}

struct ChunkLru {
    entries: VecDeque<CachedChunk>,
}

impl ChunkLru {
    fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_CACHED_CHUNKS),
        }
    }

    fn get(&mut self, set: &str, chunk: u16) -> Option<Arc<[u8]>> {
        let pos = self
            .entries
            .iter()
            .position(|entry| entry.set == set && entry.chunk == chunk)?;
        let entry = self.entries.remove(pos)?;
        let data = Arc::clone(&entry.data);
        self.entries.push_back(entry);
        Some(data)
    }

    fn insert(&mut self, set: String, chunk: u16, data: Arc<[u8]>) {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|entry| entry.set == set && entry.chunk == chunk)
        {
            self.entries.remove(pos);
        } else if self.entries.len() >= MAX_CACHED_CHUNKS {
            self.entries.pop_front();
        }
        self.entries.push_back(CachedChunk { set, chunk, data });
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn contains(&self, set: &str, chunk: u16) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.set == set && entry.chunk == chunk)
    }
}

/// Resolves icon specs against bundled sets, verified downloaded sets, and user icon files.
pub struct Registry {
    bundled: Option<Pack<'static>>,
    downloaded: RwLock<Vec<DownloadedPack>>,
    cache: Mutex<ChunkLru>,
}

impl Registry {
    /// Opens a registry backed by the given static `NVIC1` pack bytes.
    pub fn new(bundled: &'static [u8]) -> Result<Self, PackError> {
        let pack = Pack::open(bundled)?;
        Ok(Self {
            bundled: Some(pack),
            downloaded: RwLock::new(Vec::new()),
            cache: Mutex::new(ChunkLru::new()),
        })
    }

    /// Creates an empty registry with no bundled or downloaded sets.
    pub fn empty() -> Self {
        Self {
            bundled: None,
            downloaded: RwLock::new(Vec::new()),
            cache: Mutex::new(ChunkLru::new()),
        }
    }

    /// Loads verified downloaded packs listed in `dir/installed.toml`, returning any warnings.
    pub fn load_downloaded(&self, dir: &Path) -> Vec<String> {
        let (loaded, warnings) = files::load_manifest_packs(dir);
        if let Some(packs) = loaded {
            *self
                .downloaded
                .write()
                .unwrap_or_else(PoisonError::into_inner) = packs;
            self.trim();
        }
        warnings
    }

    /// Resolves `spec` using packs and `icons_dir`, returning the resolved icon and optional warning.
    pub fn resolve(&self, spec: &IconSpec, icons_dir: &Path) -> (Resolved, Option<String>) {
        match spec {
            IconSpec::Set { set, name, tone } => self.resolve_set(set, name, *tone),
            IconSpec::Brand { name, colour } => self.resolve_brand(name, *colour),
            IconSpec::Letters {
                letters,
                shape,
                tone,
            } => (
                Resolved::Letters {
                    letters: letters.clone(),
                    shape: *shape,
                    tone: *tone,
                },
                None,
            ),
            IconSpec::Glyph { glyph, tone, dot } => match self.lookup("lucide", glyph) {
                Some((_, svg)) => (
                    Resolved::Glyph {
                        svg,
                        tone: *tone,
                        dot: *dot,
                    },
                    None,
                ),
                None => (
                    Resolved::Missing { dashed: false },
                    Some(format!("glyph `{glyph}` not found")),
                ),
            },
            IconSpec::Image { file } => files::resolve_image(file, icons_dir),
            IconSpec::Svg { file, mono } => files::resolve_svg(file, *mono, icons_dir),
        }
    }

    /// Returns the raw SVG body bytes for `(set, name)`, using the 8-chunk LRU cache.
    pub fn body(&self, set: &str, name: &str) -> Option<Arc<[u8]>> {
        self.lookup(set, name).map(|(_, svg)| svg)
    }

    /// Returns metadata for all bundled and currently loaded downloaded sets.
    pub fn sets(&self) -> Vec<SetInfo> {
        let mut out = Vec::new();
        if let Some(pack) = &self.bundled {
            out.extend(pack.sets().iter().cloned());
        }
        let downloaded = self
            .downloaded
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        for pack in downloaded.iter() {
            out.extend(pack.sets.iter().cloned());
        }
        out
    }

    /// Decompresses the newline-delimited tag list for `set`, if available.
    pub fn tags(&self, set: &str) -> Option<String> {
        if let Some(pack) = &self.bundled
            && let Ok(tags) = pack.tags(set)
        {
            return Some(tags);
        }
        let bytes = self.downloaded_bytes(set)?;
        Pack::open(&bytes).ok()?.tags(set).ok()
    }

    /// Empties the decoded-chunk LRU cache to release memory.
    pub fn trim(&self) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn resolve_set(&self, set: &str, name: &str, tone: Option<Tone>) -> (Resolved, Option<String>) {
        if let Some((_, svg)) = self.lookup(set, name) {
            return (
                Resolved::Mono {
                    svg,
                    tone,
                    brand: None,
                },
                None,
            );
        }
        if DOWNLOAD_SET_IDS.contains(&set) && !self.has_set(set) {
            return (
                Resolved::Missing { dashed: true },
                Some(format!("icon set `{set}` is not installed")),
            );
        }
        (
            Resolved::Missing { dashed: false },
            Some(format!("icon `{set}:{name}` not found")),
        )
    }

    fn resolve_brand(&self, name: &str, colour: BrandColour) -> (Resolved, Option<String>) {
        let Some((icon_ref, svg)) = self.lookup("brands", name) else {
            return (
                Resolved::Missing { dashed: false },
                Some(format!("brand icon `{name}` not found")),
            );
        };
        let brand = match colour {
            BrandColour::Brand => icon_ref.brand,
            BrandColour::Theme => None,
        };
        (
            Resolved::Mono {
                svg,
                tone: None,
                brand,
            },
            None,
        )
    }

    fn has_set(&self, set: &str) -> bool {
        if let Some(pack) = &self.bundled
            && pack.sets().iter().any(|info| info.id == set)
        {
            return true;
        }
        let downloaded = self
            .downloaded
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        downloaded
            .iter()
            .any(|pack| pack.sets.iter().any(|info| info.id == set))
    }

    fn downloaded_bytes(&self, set: &str) -> Option<Arc<[u8]>> {
        let downloaded = self
            .downloaded
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        downloaded
            .iter()
            .find(|pack| pack.sets.iter().any(|info| info.id == set))
            .map(|pack| Arc::clone(&pack.bytes))
    }

    fn lookup(&self, set: &str, name: &str) -> Option<(IconRef, Arc<[u8]>)> {
        if let Some(pack) = &self.bundled
            && let Some(icon_ref) = pack.find(set, name)
        {
            let chunk = self.cached_chunk(set, icon_ref.chunk, pack)?;
            let slice = pack::body(&chunk, &icon_ref)?;
            return Some((icon_ref, Arc::from(slice)));
        }
        let bytes = self.downloaded_bytes(set)?;
        let pack = Pack::open(&bytes).ok()?;
        let icon_ref = pack.find(set, name)?;
        let chunk = self.cached_chunk(set, icon_ref.chunk, &pack)?;
        let slice = pack::body(&chunk, &icon_ref)?;
        Some((icon_ref, Arc::from(slice)))
    }

    fn cached_chunk(&self, set: &str, chunk: u16, pack: &Pack<'_>) -> Option<Arc<[u8]>> {
        if let Some(hit) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(set, chunk)
        {
            return Some(hit);
        }
        let decoded: Arc<[u8]> = Arc::from(pack.chunk(set, chunk).ok()?);
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(set.to_owned(), chunk, Arc::clone(&decoded));
        Some(decoded)
    }

    #[cfg(test)]
    pub(crate) fn cached_chunks(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    #[cfg(test)]
    pub(crate) fn has_cached_chunk(&self, set: &str, chunk: u16) -> bool {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(set, chunk)
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Returns the process-wide icon registry opened from [`BUNDLED`].
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| match Registry::new(BUNDLED) {
        Ok(reg) => reg,
        Err(err) => {
            tracing::error!(error = %err, "failed to open bundled icon pack");
            Registry::empty()
        }
    })
}

#[cfg(test)]
mod tests;
