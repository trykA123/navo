use std::rc::Rc;
use std::sync::Arc;

use rusty_core::Location;
use rusty_ui::prelude::*;
use rusty_ui::window_view::{Attach, FileHooks, FileText, Host, Kinds, Sniff, ViewerBody};

pub const TREE_NAME: &str = "ferrite.json";
pub const TABLE_NAME: &str = "cities.csv";
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

pub const MARKDOWN: &str = "# Ferrite 0.0.1

A fast, keyboard-first file manager. This release adds **structured views** to F3.

## What is new

- **Tree** for JSON, YAML and XML, with the path of the selected node in the header.
- **Table** for CSV and TSV, with a sticky header and numeric sorting.
- **Markdown**, rendered. Press `1` to see the source.

> Large ARXML files are indexed while you read: the tree opens before the scan ends.

## Keys

| Key | Does |
| --- | --- |
| `Ctrl+Shift+C` | Copies the path of the node |
| `Ctrl+F3` | Sorts by the cursor column |

```rust
let tree = parse_json(bytes)?;
```

Links open on Ctrl+click: [project page](https://example.com/ferrite).
";

fn hooks(bytes: &'static str) -> FileHooks {
    FileHooks {
        read: Arc::new(move |_| Ok(FileText::plain(bytes))),
        bytes: Arc::new(move |_, _| Ok(bytes.as_bytes().to_vec())),
        range: Arc::new(move |_, offset, len| {
            let data = bytes.as_bytes();
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
    let location = Location::new(format!("/samples/{name}")).ok()?;
    let sniff = Sniff {
        head: text.as_bytes(),
        name,
        len: text.len() as u64,
    };
    let kind = Kinds::builtin().choose(&sniff).into_iter().next()?;
    let hooks = hooks(text);
    let loaded = kind.load(&hooks, &location, &sniff).ok()?;
    let host = Host {
        title: name.into(),
        location,
        hooks,
        on_saved: Rc::new(|_| {}),
        open: Rc::new(|_, _| {}),
    };
    let attach = Attach {
        loaded,
        host,
        edit: false,
    };
    kind.attach(attach, window, cx).ok()
}
