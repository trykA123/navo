# Rusty

A keyboard-first file manager for Windows, written in Rust. Total Commander's speed and density, plus tiled panes, saved sets of panels, pins, a configurable drawer (transfers, clipboard, search, compare, terminal and more), fast fuzzy search and plugins.

Status: design and architecture decided. Foundation phase next; no application code yet.

- [Discussion so far](docs/discussion.md), including why Rust
- [Architecture](docs/architecture.md): crates, state, caching, start-up, safety, terminal
- [Plugins](docs/plugins.md): what they are, permissions, lifecycle, how to build one
- [Decisions](docs/decisions.md)
- [Roadmap](docs/roadmap.md): phases from foundation to v2.0; visual version in [`design/roadmap.html`](design/roadmap.html)
- Interactive prototypes: open [`design/prototype/picker.html`](design/prototype/picker.html) in a browser (chosen direction: **B**), or the single-file [`design/commander-bundle.html`](design/commander-bundle.html).
- Screenshots of the chosen design: [`design/screens`](design/screens)

![Commander design](design/screens/b4-1440.png)
