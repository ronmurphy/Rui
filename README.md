<img width="152" height="152" alt="Rui" src="https://github.com/user-attachments/assets/219da171-2664-4761-8ae0-f6744c846aa5" />

# Rui — A GTK4 UI Designer for Rust
<img width="1761" height="962" alt="Screenshot_20260311_003053" src="https://github.com/user-attachments/assets/e9055e57-5570-4523-8b03-d0146cc7c76e" />

> **Status: Early Foundation** — This project is being built as a starting point, not a long-term commitment. If you find it useful and want to take it further, we're actively looking for contributors and future maintainers.

Rui (said like "Rooey" — **R** for Rust + **UI**) is a native GTK4 application for visually designing `.ui` files used by GTK/Libadwaita Rust applications. It aims to be the missing GUI builder in the Rust desktop ecosystem.

## What It Does (v0.3)

**Editor**
- Tabbed editor with syntax highlighting (GtkSourceView 5) for `.ui`, `.rs`, `.py`, `.js`, `.ts`, `.sh`, and HTML/CSS
- File tree sidebar and minimap
- Find & Replace with regex, Go to Line
- Undo/Redo — full designer history with crash recovery
- Session persistence (remembers open files and layout)
- Built-in dark theme (Catppuccin Mocha); follows system GTK theme

**GTK4 UI Designer**
- **Live Canvas** — renders `.ui` XML in a side pane as you type (500 ms debounce)
- **Widget Palette** — 30+ GTK4 widgets across 5 categories, click to insert XML
- **Property Inspector** — edit widget properties visually, writes back to XML in real-time
- **GtkGrid Designer** — right-click to set column/row/span; merge-mode (select cells → apply span)
- **Drag & Drop** — drag widgets on the canvas to reorder; drag from palette to insert
- **Code Generation** — emit idiomatic Rust signal handler stubs from `.ui` files
- **Template Library** — reusable UI snippets to jump-start new layouts
- **New Project Dialog** — scaffold a project with grid dimension input (rows × cols)

**Build & Run**
- Run scripts directly from the editor (F5) — Python, JavaScript, TypeScript, Rust, Shell, HTML/CSS
- Build Rust projects (`cargo build --release`) with a `cargo check` pre-flight that catches errors before the full build
- Build Install — builds a release binary and generates a `.desktop` launcher + icon for system installation
- Stop button (Shift+F5) to kill a running process

**AI Integration**
- **Claude Code Panel** — embedded Claude Code assistant with live `.ui` + companion `.rs` context; apply code blocks directly to files
- **API AI Chat Panel** — multi-provider chat (OpenAI, Anthropic, Gemini, OpenAI-compat); streams responses with Apply buttons; configured via ⚙ gear button

**Layouts**
- Code View (`Ctrl+1`) — full sidebar, editor with minimap, no canvas
- Designer View (`Ctrl+2`) — canvas + toolbox front and centre, narrow sidebar
- All panels toggleable independently (sidebar, output, canvas, toolbox, preview, minimap)

## Building

```sh
# Prerequisites: GTK4, GtkSourceView 5, WebKitGTK 6 (optional)
# On Arch:    sudo pacman -S gtk4 gtksourceview5 webkitgtk-6.0
# On Fedora:  sudo dnf install gtk4-devel gtksourceview5-devel webkit2gtk6.0-devel
# On Ubuntu:  sudo apt install libgtk-4-dev libgtksourceview-5-dev libwebkitgtk-6.0-dev

cargo build --release
# Binary at target/release/rui
```

Without WebKit (disables AI chat panel and HTML preview):

```sh
cargo build --release --no-default-features
```

Or use the provided script which checks dependencies and installs the binary, icon, and desktop entry:

```sh
./build.sh
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
autosave = false
```

Available colour schemes: `rui-theme`, `classic`, `oblivion`, `solarized-dark`, `solarized-light`, `kate`, `tango`, `cobalt`.

## License

MIT — see [LICENSE](LICENSE).

## Contributing

Contributions welcome! See [ARCHITECTURE.md](ARCHITECTURE.md) for the module layout. The codebase is intentionally simple — one binary crate, no macros, straightforward GTK4.

If you're interested in becoming a maintainer, open an issue.
