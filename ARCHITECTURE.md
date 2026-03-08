# Rui Architecture

Single binary crate. No proc macros, no workspace, no code generation at build time.

## Module Map

```
src/
├── main.rs        — Entry point, GTK Application setup, CSS loading
├── config.rs      — EditorConfig + RuiConfig, reads ~/.config/rui/rui.toml
├── schemes.rs     — Generates GtkSourceView color scheme XML (Catppuccin Mocha)
├── app.rs         — AppState struct, window builder, action wiring (THE HUB)
├── tab.rs         — EditorTab: one sourceview per file, language detection
├── notebook.rs    — NotebookManager: tab lifecycle (open, close, switch, reorder)
├── sidebar.rs     — File tree (TreeStore with lazy-load expansion)
├── find.rs        — Find/replace bar with regex toggle
├── output.rs      — Bottom panel: Output / Problems / Run tabs
├── runner.rs      — Process execution (cargo, python, node, shell)
├── statusbar.rs   — Language label, cursor position, modified indicator
├── menubar.rs     — GMenu builder for the header bar
├── goto.rs        — "Go to Line" dialog
├── help.rs        — Keyboard shortcuts / help dialog
├── session.rs     — Save/load open files across restarts
├── diff_tool.rs   — Apply unified diffs from AI responses
├── preview.rs     — WebKit HTML/CSS preview pane (feature: "preview")
└── ai_panel.rs    — AI chat window with persistent sessions (feature: "preview")
```

## Future Modules (Not Yet Created)

These are the Rui-specific modules that will differentiate it from a plain editor:

```
src/
├── palette.rs     — Widget palette panel (drag GTK4 widgets into design)
├── inspector.rs   — Property inspector (edit selected widget's properties)
├── canvas.rs      — Live preview: render .ui XML in a GTK pane
└── codegen.rs     — Generate Rust bindings from .ui files
```

## Data Flow

```
main.rs
  └─> app::build_ui()
        ├─> config::load()          → EditorConfig
        ├─> sidebar::build()        → file tree
        ├─> notebook::build()       → tabbed editor area
        ├─> find::build()           → search bar
        ├─> output::build()         → bottom panel
        ├─> statusbar::build()      → status bar
        ├─> menubar::build()        → header menus
        └─> action wiring           → connects everything
```

## Key Types

| Type | Module | Purpose |
|------|--------|---------|
| `AppState` | app.rs | Holds all UI references, the config, and the file watcher |
| `EditorConfig` | config.rs | Deserialized editor preferences |
| `EditorTab` | tab.rs | One open file — view, buffer, path, language |
| `NotebookManager` | notebook.rs | Manages the tab strip |
| `OutputPanel` | output.rs | Bottom pane with multiple text views |
| `RunManager` | runner.rs | Spawns and tracks child processes |

## Collaboration Guide

To avoid merge conflicts, each new feature should live in its own file:

- **Widget Palette** → `palette.rs` — self-contained panel that emits XML snippets
- **Property Inspector** → `inspector.rs` — reads/writes attributes on selected XML nodes
- **Live Canvas** → `canvas.rs` — parses .ui XML and renders a GTK widget tree
- **Code Generation** → `codegen.rs` — walks .ui XML and emits Rust code

Integration point: `app.rs` wires new panels into the layout. Keep `app.rs` changes minimal (add one field to `AppState`, one call in `build_ui`).

## Config & Data Paths

| Path | Purpose |
|------|---------|
| `~/.config/rui/rui.toml` | User config |
| `~/.local/share/rui/session.json` | Open file list |
| `~/.local/share/rui/ai-session/` | AI chat cookies/storage |
| `~/.local/share/gtksourceview-5/styles/rui-theme.xml` | Color scheme |
