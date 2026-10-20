use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_ui::prelude::*;
use rusty_ui::window_view::{
    Attach, FileHooks, FileText, Head, Host, Kinds, Load, Sniff, ViewerBody,
};

pub const TREE_NAME: &str = "ferrite.json";
pub const TABLE_NAME: &str = "cities.csv";
pub const CODE_NAME: &str = "copy_job.rs";
pub const MARKDOWN_NAME: &str = "RELEASE.md";

pub const TREE: &str = r#"{
  "name": "ferrite",
  "version": "0.0.1",
  "private": true,
  "viewer": {
    "kinds": ["text", "hex", "tree", "table", "markdown"],
    "limits": {"edit_mb": 16, "tree_mb": 64, "xml_index": "streamed"},
    "keys": {"text": "1", "hex": "3", "rich": "5"}
  },
  "panes": [
    {"path": "C:\\Users\\admin\\Documents", "sort": "name", "hidden": false},
    {"path": "D:\\Projects\\ferrite", "sort": "date", "hidden": true}
  ],
  "theme": null,
  "font_size": 13.5
}"#;

pub const TABLE: &str = "city;county;people;area_km2;founded;note
Cluj-Napoca;Cluj;286598;179.5;1213;\"University town; tech hub\"
Iasi;Iasi;271692;93.9;1408;Cultural capital
Timisoara;Timis;250849;130.5;1212;First electric street lights in Europe
Constanta;Constanta;263688;124.89;-600;Oldest continuously inhabited city
Brasov;Brasov;237589;267.32;1235;Black Church
Craiova;Dolj;234140;81.41;1475;
Galati;Galati;217851;246.4;1445;Danube port
Oradea;Bihor;183105;115.56;1113;Art Nouveau centre
Ploiesti;Prahova;180540;58.2;1596;Oil refining
Braila;Braila;154686;43.0;1368;
Arad;Arad;145078;46.18;1028;
Suceava;Suceava;124161;52.0;1388;
Sibiu;Sibiu;134309;121.0;1191;European Capital of Culture 2007
Bacau;Bacau;136087;43.1;1408;
Targu Mures;Mures;116033;49.3;1332;
Baia Mare;Maramures;108759;235.7;1329;
Buzau;Buzau;100230;81.3;1431;
";

pub const CODE: &str = r##"/// Copies one file and reports progress to the job queue.
#[derive(Debug, Clone)]
pub struct CopyJob<'a> {
    pub source: &'a Path,
    pub chunk: usize,
}

impl<'a> CopyJob<'a> {
    pub const CHUNK: usize = 1 << 20;

    pub fn run(&self, tx: &Sender<Progress>) -> io::Result<u64> {
        let mut done = 0u64; // bytes so far
        let pattern = Regex::new(r"^\d{4}-\d{2}")?;
        if self.chunk == 0 || !self.source.exists() {
            return Err(io::Error::other("empty \"source\""));
        }
        let name = self.source.display().to_string();
        tx.send(Progress::Started { total: None }).ok();
        for part in name.split('/').filter(|part| pattern.is_match(part)) {
            done += part.len() as u64;
        }
        Ok(done)
    }
}
"##;

pub const MARKDOWN: &str = "# Ferrite 0.0.1

A fast, **keyboard-first** file manager with *structured views* in F3. ~~Beta~~ Alpha, see the [project page](https://example.com/ferrite) and [setup notes](docs/setup.md)[^1].

> Large ARXML files are indexed while you read.
>
> > The tree opens before the scan ends.

## What is new

- **Tree** for JSON, YAML and XML
  - with the path of the selected node
- **Table** for CSV and TSV
1. Press `1` for the source
2. Press `5` for this view
- [x] Rendered Markdown
- [ ] PDF pages

| Key | Does | Since |
| :-- | :--: | ----: |
| `Ctrl+Shift+C` | Copies the node path | 0.0.1 |
| `Ctrl+F3` | Sorts by the cursor column | 0.0.1 |

## Tiers

| Tier | Model | Roles | Used for |
| --- | --- | --- | --- |
| **T1** | Opus 5.5, `medium` | `patcher`, `infra-builder` | Moves, exact edits, small config, scaffolds |
| **T2** | Opus 5.5, `medium` | `ui-builder`, `checker` | UI work, medium implementation, writing the failing tests, CI |
| **T4** | Opus 5.5, `high` | orchestrator, `ui-builder-high` | The core and the hardest atoms: everything that can lose user data, the pane view, hard refactors |

```rust
let tree = parse_json(bytes)?; // a long line scrolls inside its block when it does not fit the width of the view
```

---

![Ferrite logo](assets/logo.png) and raw <kbd>HTML</kbd> stay literal.

<details>
<summary>Raw HTML block</summary>
</details>

[^1]: Links open with Tab and Enter, or Ctrl+click.
";

fn hooks(bytes: &'static [u8]) -> FileHooks {
    let text = std::str::from_utf8(bytes).unwrap_or_default();
    FileHooks {
        read: Arc::new(move |_| Ok(FileText::plain(text))),
        bytes: Arc::new(move |_, _| Ok(bytes.to_vec())),
        range: Arc::new(move |_, offset, len| {
            let data = bytes;
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(data.len());
            Ok(data
                .get(start..(start + len).min(data.len()))
                .unwrap_or_default()
                .to_vec())
        }),
        ..FileHooks::none()
    }
}

pub fn body(
    name: &'static str,
    text: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    body_in((name, text.as_bytes()), None, window, cx)
}

pub fn binary(
    name: &'static str,
    bytes: &'static [u8],
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    body_in((name, bytes), None, window, cx)
}

fn beside(bytes: &'static [u8], (sibling, text): (&'static str, &'static str)) -> FileHooks {
    let files = hooks(bytes);
    let whole = files.bytes.clone();
    FileHooks {
        head: Arc::new(move |location| {
            if location.as_str().ends_with(sibling) {
                Ok(Head {
                    bytes: text.as_bytes().to_vec(),
                    len: text.len() as u64,
                })
            } else {
                Err("not found".to_owned())
            }
        }),
        bytes: Arc::new(move |location, limit| {
            if location.as_str().ends_with(sibling) {
                Ok(text.as_bytes().to_vec())
            } else {
                whole(location, limit)
            }
        }),
        ..files
    }
}

pub fn binary_beside(
    (name, bytes): (&'static str, &'static [u8]),
    sibling: (&'static str, &'static str),
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    attach((name, bytes), (None, beside(bytes, sibling)), window, cx)
}

pub fn editing(
    name: &'static str,
    text: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    body_in((name, text.as_bytes()), Some("text"), window, cx)
}

fn body_in(
    (name, text): (&'static str, &'static [u8]),
    edit_kind: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    attach((name, text), (edit_kind, hooks(text)), window, cx)
}

fn attach(
    (name, text): (&'static str, &'static [u8]),
    (edit_kind, hooks): (Option<&str>, FileHooks),
    window: &mut Window,
    cx: &mut App,
) -> Option<Box<dyn ViewerBody>> {
    let location = Location::new(format!("/samples/{name}")).ok()?;
    let sniff = Sniff {
        head: text,
        name,
        len: text.len() as u64,
    };
    let kind = Kinds::builtin()
        .choose(&sniff)
        .into_iter()
        .find(|kind| edit_kind.is_none_or(|id| kind.id() == id))?;
    let cancel = AtomicBool::new(false);
    let loaded = kind
        .load(Load::new(&hooks, &location, &sniff, &cancel))
        .ok()?;
    let host = Host {
        title: name.into(),
        location,
        hooks,
        carried: None,
        is_preview: false,
        on_saved: Rc::new(|_| {}),
        open: Rc::new(|_, _| {}),
    };
    let attach = Attach {
        loaded,
        host,
        edit: edit_kind.is_some(),
    };
    kind.attach(attach, window, cx).ok()
}
