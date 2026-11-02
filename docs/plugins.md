# Navo plugins

Status: proposal for review, 13 October 2026.

## What a plugin is

A plugin is a package that adds to Navo through fixed extension points. It cannot change Navo's code, draw its own pixels, or reach the file system, network or other programs unless the user granted that.

There are two kinds:

| Kind | Contains | Can run code | Examples |
| --- | --- | --- | --- |
| **Package** | Data only (TOML, images) | No | Themes, keymaps, command macros, column presets, file-type icons |
| **Extension** | A WebAssembly component plus a manifest | Yes, sandboxed | Previewers, drawer views, remote file systems, search sources, columns, commands |

Macros in packages are sequences of built-in commands (`pane.open_tab`, `path.copy`, `terminal.open`), which covers most personal automation without code. There is no separate scripting language in v1; that keeps one model and one security boundary.

## Extension points

| Point | What it adds | Example |
| --- | --- | --- |
| Command | A new command in the palette, keymaps and menus | “Copy as Markdown link” |
| Drawer view | A view in the drawer | Docker containers for the current folder |
| Previewer | Preview for file types | Markdown, `.blend`, `.psd` |
| Column | Per-file data in panels | Image dimensions, audio length, git author |
| Context action | Items in the file context menu | “Upload to NAS” |
| File system | A browsable location | SFTP, S3, an archive format |
| Search source | Results in Ctrl+K | Bookmarks from a notes app; the optional **Everything** module |
| Theme, keymap, icons | Look and keys (packages only) | Total Commander keymap, Nord theme |
| Metadata provider | Per-file fields that feed columns, sorting, filters and search syntax at once | EXIF: a Camera column and `exif.camera:sony` |
| View mode | A new way to show a folder | Gallery, Miller columns, tree, timeline |
| Transform | Turns files into other files; the output goes through the job engine with a plan, progress and undo | Resize images, PDF to images, video to GIF |
| Automation rule | Runs commands when something happens; every command goes through the usual plans and confirmations | Sort new files in Downloads |
| Status and drive badge | Small items in the status bar or on drive buttons | SMART health, cloud sync state |

Plugins cannot hook into file operations (no “before delete” hooks) and cannot replace built-in safety confirmations.

## How plugin UI works

Extensions describe their UI with a small set of widgets (list, table, text, key-value, progress, image from bytes, button, input, toggle). Navo renders them with its own components and theme. This keeps every plugin consistent with the app, accessible from the keyboard, and unable to imitate Navo's own confirmation dialogs.

## The manifest

```toml
[plugin]
id = "dev.example.image-columns"
name = "Image columns"
version = "0.3.1"
api = "0.1"                  # host API (WIT world) version
entry = "image_columns.wasm"

[permissions]
fs.read = ["${selection}", "${pane}"]   # only what the user is looking at
fs.write = []                           # nothing
network = []                            # no hosts
process = []                            # no programs
clipboard.write = false

[[contributes.column]]
id = "dimensions"
title = "Dimensions"
extensions = ["jpg", "png", "webp"]

[settings]
max_megapixels = { type = "int", default = 50, min = 1, max = 400 }
```

Permission scopes are narrow by default: `${selection}`, `${pane}`, a named folder, or a host name. Broad access (`fs.read = ["*"]`) is shown in red at install time.

## Lifecycle

1. **Install:** from a folder or `.navoplugin` archive into `%APPDATA%\Navo\plugins\<id>`. Navo validates the manifest, records the package hash and shows the permissions in plain words. Nothing runs until the user approves.
2. **Load lazily:** an extension starts when one of its contributions is first used (its column becomes visible, its command runs, its previewer is needed), not at Navo's start.
3. **Run in a sandbox:** each extension gets its own wasmtime instance with a memory limit (default 64 MB), CPU deadlines (epoch interruption), and only the host functions its permissions allow.
4. **Unload when idle:** after 5 minutes unused; state the plugin wants to keep goes through a small key-value store the host provides.
5. **Fail safely:** a trap, timeout or permission error disables the contribution, logs it in the Activity view, and leaves Navo running.
6. **Update:** a new version shows the permission difference and needs approval again if permissions grew. The previous version is kept for rollback.

## The host boundary

- Every host call checks the grant, canonicalises paths and keeps them inside the granted scope.
- File writes go through Navo's own job engine, so they get plans, confirmations, verification and undo. Permanent deletion is not available to plugins.
- Network access is limited to the listed hosts, through the host's HTTP client with timeouts and size limits.
- Starting programs uses an allow-list from the manifest, with argument lists only, and asks the user the first time.
- Calls are rate-limited, and large data (thumbnails, file contents) is passed with explicit size limits.

## Making a plugin

- **Rust first:** a `navo-plugin-sdk` crate with types and helpers over the WIT world, and `cargo navo-plugin new <name>` for a template with a test.
- **Other languages:** anything that compiles to a WASM component against the published WIT files (Go, C#, JS via componentize-js), without an SDK at first.
- **Development mode:** `navo --plugin-dev <folder>` loads an unpacked plugin, reloads it on rebuild, and shows its logs in the Activity view.
- **Testing:** a fake host crate runs plugins in unit tests with chosen permissions and fake files.
- **Sharing:** a plain folder or archive in v1. A registry (a git repository index with signed releases) can come later.

## First-party plugins

Some features ship as plugins so they can be turned off or replaced:

- **Everything (optional):** global search through voidtools' Everything for people who already run it; Navo's own index is the default (D23). When Everything is missing, the module can offer to install it: it downloads the official installer from voidtools.com, verifies its checksum, and runs it only after the user agrees. Needs the `network` permission for voidtools.com and `process` for the installer.

## Module ideas and packs

Navo works like a Framework laptop: the core is the mainboard, slots are the expansion ports, and modules are the cards people add (D24). Examples of what users could build:

| For | Modules |
| --- | --- |
| Developers | Git, Docker containers for the current folder, task runner (npm scripts, `make`, `cargo`), open in IDE, JSON/YAML/Markdown preview, pull-request status column |
| Photo and video | EXIF columns, RAW preview, photo map, bulk resize and convert, video duration and codec, similar-image duplicates, audio tags |
| Homelab and sysadmin | SFTP, S3/MinIO, WebDAV, SMART health on the drive bar, permission (ACL) viewer, hash checks, Wake-on-LAN, media-server library refresh |
| Documents | PDF merge and split, Office preview, OCR with text search, document tags |
| Cloud | OneDrive, Google Drive and Dropbox sync state as a column and drive badge |
| Productivity | Folder notes, Downloads sorter, send to phone over the local network, screenshot organiser |
| Security | Hash lookup on VirusTotal, encrypted vaults (VeraCrypt, age), Mark-of-the-Web viewer |
| Local AI | Semantic search with local models, rename suggestions, photo tagging; network only with explicit permission |

**Example plugin: weekly Downloads folders.** The first small plugin to show how plugins work. It watches the Downloads folder. A new file or folder moves into a folder for its week, for example `2026-10-19 – 2026-10-25`. It needs one folder watch, one move per item and a setting for the folder and the week start. It shows each move in the activity strip and can be undone like any other move. Idea from the owner, 2026-10-21.

**Module packs** (Developer, Photo, Homelab) install a curated set in one step and show every permission first.

## Versioning

- The host API is a versioned WIT world (`navo:plugin@0.1`). Navo supports the current and the previous minor version.
- Breaking changes go into a new version with a migration note, never silently.

## Built-in features use the same points

Transfers, Compare, Git, Notes and the other drawer views, the built-in previewers and the columns are implemented against the same extension-point traits inside the app. That keeps the extension points real, tested and expressive enough for third parties.
