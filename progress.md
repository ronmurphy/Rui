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
- Replaces separate palette and inspector panels in the layout

#### Layout Modes
- **Code View** (Ctrl+1): Full sidebar, minimap, output — no canvas/toolbox
- **Designer View** (Ctrl+2): Sidebar, canvas, toolbox visible — no minimap
- Layout choice persists across sessions (saved in session.json)

#### File → New .ui File
- Creates a new tab with a boilerplate GTK4 `.ui` XML skeleton
- Auto-sets XML syntax highlighting
- Immediately activates canvas and toolbox

#### Session Persistence
- Remembers open files AND last active layout (code/designer)
- Gracefully handles old session files (defaults to code view)

---

## File Inventory (22 source files)

| File | Lines | Purpose |
|------|-------|---------|
| `main.rs` | Entry point, module declarations, CSS loading |
| `config.rs` | Standalone config (no rdm-common dependency) |
| `schemes.rs` | Color scheme XML generation |
| `app.rs` | AppState, window builder, all action wiring (THE HUB) |
| `tab.rs` | EditorTab — one sourceview per file |
| `notebook.rs` | Tab lifecycle management |
| `sidebar.rs` | File tree panel |
| `find.rs` | Find/replace bar |
| `output.rs` | Bottom output panel |
| `runner.rs` | Process execution |
| `statusbar.rs` | Status bar |
| `menubar.rs` | Menu bar builder |
| `goto.rs` | Go to Line dialog |
| `help.rs` | Shortcuts/help dialog |
| `session.rs` | Session save/load (files + layout) |
| `diff_tool.rs` | Unified diff application |
| `preview.rs` | WebKit HTML/CSS preview |
| `ai_panel.rs` | AI chat panel |
| `canvas.rs` | Live .ui preview renderer |
| `palette.rs` | Widget palette (30 widgets) |
| `inspector.rs` | Property inspector |
| `toolbox.rs` | Palette + Inspector combined panel |

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

- [ ] Test fresh clone + build on main laptop
- [ ] `codegen.rs` — Generate Rust bindings from `.ui` files
- [ ] Drag-and-drop widget placement on canvas (Tier 2)
- [ ] Widget reordering in the XML tree
- [ ] Undo/redo for inspector property edits
- [ ] Theme selector (beyond Catppuccin Mocha)
- [ ] Contributors welcome — see ARCHITECTURE.md for module guide
