# Rui — Progress Log

## What Is Rui

Rui ("Rooey") is a standalone GTK4 `.ui` file designer for Rust developers.
Forked from rdm-editor, now its own project with its own identity.

- **Language:** Rust 2021
- **UI toolkit:** GTK4 0.9 (v4_10), GtkSourceView 5, WebKit 6 (optional)
- **License:** MIT
- **Repo:** https://github.com/ronmurphy/Rui

---

## Completed Features

### Core Editor (inherited from rdm-editor fork)
- Tabbed source editor with GtkSourceView 5 syntax highlighting
- File tree sidebar with lazy-load expansion
- Find/replace bar with regex toggle
- Go to Line dialog (Ctrl+G)
- Output panel (Output / Problems / Run tabs)
- Script runner (cargo, python, node, shell)
- Status bar (language, cursor position, modified indicator)
- Full menu bar with keyboard shortcuts
- Drag-and-drop file opening
- Built-in Catppuccin Mocha dark theme (auto-installed)
- AI chat panel with persistent sessions (feature-gated behind "preview")
- HTML/CSS live preview via WebKit (feature-gated behind "preview")
- Diff tool for applying unified diffs from AI

### Rui-Specific Features (new)

#### Live Canvas Preview — `canvas.rs`
- Parses `.ui` XML via `gtk4::Builder::from_string()` and renders widgets in a side pane
- 500ms debounce via generation counter (avoids partial-XML crashes)
- Auto-shows when a `.ui` file is opened, hides for non-`.ui` files
- 50/50 horizontal split with the editor (dynamic, user-resizable)
- Single-click selects widget (fires on release so DragSource gets priority)
- Double-click opens/creates companion `.rs` handler file
- Right-click GtkBox children: Move Up/Down/Into/Delete popover
- Right-click GtkGrid children: column/row/span spinbuttons + Apply + Delete
- Drag widgets to reorder (DragSource in Capture phase for interactive widgets)
- GtkGrid empty cells render as Frame placeholders (DropTarget) up to max+1 boundary

#### Widget Palette — `palette.rs`
- 30 GTK4 widgets across 5 categories:
  - Containers (8): Box, Grid, Stack, Notebook, Paned, Frame, ScrolledWindow, Overlay
  - Display (6): Label, Image, Spinner, ProgressBar, LevelBar, Separator
  - Input (10): Button, ToggleButton, CheckButton, Switch, Entry, PasswordEntry, SpinButton, Scale, DropDown, ComboBoxText
  - Text (2): TextView, SearchEntry
  - Layout (3): CenterBox, FlowBox, ListBox
- Click to insert XML snippet at cursor, wrapped in `<child>` tags
- Auto-indents to match current cursor indentation

#### Property Inspector — `inspector.rs`
- Parses XML to find `<object>` block at cursor position
- Shows widget class, id, and all `<property>` elements as editable rows
- Editing a property value in the inspector writes back to the XML buffer in real-time
- Updates automatically on cursor movement via `cursor-position-notify`

#### Toolbox Panel — `toolbox.rs`
- Combines palette (top) and inspector (bottom) in a single vertical Paned
- 50/50 default split, user-resizable
- One toggle shortcut (Ctrl+Shift+T), one menu item — reduces panel clutter

#### Code Generation — `codegen.rs`
- Run → Generate All Handlers: emits Rust handler stubs for every signal in the `.ui` file
- Double-click widget on canvas: opens/creates companion `-app.rs` file with handler skeleton
- Companion file path derived from `.ui` path (e.g. `layout.ui` → `layout-app.rs`)

#### Designer History & Undo/Redo — `history.rs`
- Every canvas edit is saved to a per-session temp dir
- Ctrl+Z / Ctrl+Y undo/redo through the full designer edit history
- Crash recovery: on next launch, Rui offers to restore the last unsaved session

#### Widget Outline Tree — `outline.rs`
- Third tab in the left sidebar panel ("Tree")
- Shows the full widget hierarchy from the current `.ui` XML
- Click a node to jump to that widget's `<object>` block in the editor

#### Claude Code Panel — `claude_code.rs`
- Embedded AI assistant that shells out to the `claude` CLI (`-p --verbose --output-format stream-json`)
- Automatically includes the current `.ui` buffer and companion `.rs` file as context in every message
- Streaming responses rendered in a scrollable chat history
- Code blocks get "Apply to .ui" / "Apply to .rs" buttons — one click writes the fix to the correct file
- Window extends to accommodate the panel on open, shrinks on close
- Running Claude process is killed cleanly on app shutdown

#### Layout Modes
- **Code View** (Ctrl+1): Full sidebar, minimap, output — no canvas/toolbox
- **Designer View** (Ctrl+2): Sidebar, canvas, toolbox visible — no minimap
- Layout choice persists across sessions (saved in session.json)

#### File → New .ui File / New Project
- New .ui File: boilerplate GTK4 XML skeleton, auto-activates canvas and toolbox
- New Project: creates a Cargo project scaffold with a `.ui` file and companion `.rs`

#### Session Persistence
- Remembers open files AND last active layout (code/designer)
- Gracefully handles old session files (defaults to code view)

---

## File Inventory (27 source files)

| File | Purpose |
|------|---------|
| `main.rs` | Entry point, module declarations, CSS loading |
| `config.rs` | Editor config (font, theme, etc.) |
| `schemes.rs` | Color scheme XML generation |
| `app.rs` | AppState, window builder, all action wiring (THE HUB) |
| `tab.rs` | EditorTab — one sourceview per file |
| `notebook.rs` | Tab lifecycle management |
| `sidebar.rs` | File tree panel |
| `find.rs` | Find/replace bar |
| `output.rs` | Bottom output panel (Output / Problems / Run) |
| `runner.rs` | Process execution (cargo, shell, etc.) |
| `statusbar.rs` | Status bar |
| `menubar.rs` | Menu bar builder |
| `goto.rs` | Go to Line dialog |
| `help.rs` | Shortcuts/help dialog |
| `session.rs` | Session save/load (files + layout) |
| `diff_tool.rs` | Unified diff application |
| `mru.rs` | Most-recently-used file/project lists |
| `preview.rs` | WebKit HTML/CSS preview (feature-gated) |
| `ai_panel.rs` | Web AI chat panel (feature-gated) |
| `canvas.rs` | Live .ui preview renderer + drag/drop designer |
| `palette.rs` | Widget palette (30 widgets, 5 categories) |
| `inspector.rs` | Property inspector |
| `toolbox.rs` | Palette + Inspector combined panel |
| `codegen.rs` | Rust handler stub generation |
| `history.rs` | Designer undo/redo + crash recovery |
| `outline.rs` | Widget hierarchy tree panel |
| `claude_code.rs` | Embedded Claude Code AI chat panel |
| `templates.rs` | Template library |

---

## Build & Install

```sh
# Install deps (Arch)
sudo pacman -S gtk4 gtksourceview5 webkit2gtk-6.0

# Build
cargo build --release

# Or use the build script (checks deps, installs desktop entry)
chmod +x build.sh && ./build.sh

# Without WebKit
cargo build --release --no-default-features
```

---

## What's Next

- [ ] Grid merge-mode UI — checkbox overlay per cell, select rectangle → apply column/row span
- [ ] New project dialog with grid dimension input (rows × cols)
- [ ] Delete selected widget with Del key
- [ ] Theme selector (beyond Catppuccin Mocha)
- [ ] Contributors welcome — see ARCHITECTURE.md for module guide
