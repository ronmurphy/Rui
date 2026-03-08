# Rui — A GTK4 UI Designer for Rust
<img width="1200" height="830" alt="Screenshot_20260308_190813" src="https://github.com/user-attachments/assets/3d1c96c5-85c1-47d5-ba91-93750e218694" />

> **Status: Early Foundation** — This project is being built as a starting point, not a long-term commitment. If you find it useful and want to take it further, we're actively looking for contributors and future maintainers.

Rui (said like "Rooey" — **R** for Rust + **UI**) is a native GTK4 application for visually designing `.ui` files used by GTK/Libadwaita Rust applications. It aims to be the missing GUI builder in the Rust desktop ecosystem.

## What It Does Today (v0.1)

- Syntax-highlighted `.ui` / XML editing (GtkSourceView 5)
- Tabbed editor with file tree sidebar
- Find/replace with regex
- Session persistence (remembers open files and layout)
- Built-in dark theme (Catppuccin Mocha)
- Run scripts directly from the editor
- AI chat panel (Claude, ChatGPT, Gemini, Codex) with persistent sessions
- **Live Canvas** — renders `.ui` XML in a side pane as you type (500ms debounce)
- **Widget Palette** — 30 GTK4 widgets across 5 categories, click to insert XML
- **Property Inspector** — edit widget properties visually, writes back to XML in real-time
- **Toolbox** — palette + inspector in one resizable panel
- **Layout Modes** — Code View (Ctrl+1) and Designer View (Ctrl+2), persisted across sessions

## What's Next

- **Code Generation** — emit idiomatic Rust bindings from `.ui` files
- **Drag-and-drop** — place widgets visually on the canvas

## Building

```sh
# Prerequisites: GTK4, GtkSourceView 5, WebKitGTK 6 (optional)
# On Arch:    sudo pacman -S gtk4 gtksourceview5 webkit2gtk-6.0
# On Fedora:  sudo dnf install gtk4-devel gtksourceview5-devel webkit2gtk6.0-devel
# On Ubuntu:  sudo apt install libgtk-4-dev libgtksourceview-5-dev libwebkitgtk-6.0-dev

cargo build --release
# Binary at target/release/rui
```

Without WebKit (disables AI chat panel and HTML preview):

```sh
cargo build --release --no-default-features
```

## Configuration

Config file: `~/.config/rui/rui.toml`

```toml
[editor]
font = "Monospace 12"
show_line_numbers = true
tab_width = 4
color_scheme = "rui-theme"
show_minimap = true
```

## License

MIT — see [LICENSE](LICENSE).

## Contributing

Contributions welcome! See [ARCHITECTURE.md](ARCHITECTURE.md) for the module layout. The codebase is intentionally simple — one binary crate, no macros, straightforward GTK4.

If you're interested in becoming a maintainer, open an issue.
