use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Label, Orientation, ScrolledWindow,
    Separator, Window,
};

pub fn show_help(parent: &gtk4::ApplicationWindow) {
    let win = Window::builder()
        .title("Rui — Help")
        .transient_for(parent)
        .modal(true)
        .default_width(640)
        .default_height(600)
        .resizable(true)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);

    let header = GtkBox::new(Orientation::Vertical, 4);
    header.set_margin_start(24);
    header.set_margin_end(24);
    header.set_margin_top(20);
    header.set_margin_bottom(12);

    let title = Label::new(Some("Rui"));
    title.set_halign(Align::Start);
    title.add_css_class("editor-help-title");
    title.set_markup("<span size='x-large' weight='bold'>Rui</span>");

    let subtitle = Label::new(Some(
        "A GTK4 UI designer for Rust developers. Edit .ui files with live preview.",
    ));
    subtitle.set_halign(Align::Start);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");

    header.append(&title);
    header.append(&subtitle);
    root.append(&header);
    root.append(&Separator::new(Orientation::Horizontal));

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    // ── Layout ──────────────────────────────────────────────────────────────
    append_section(&content, "Layout");
    append_text(&content,
        "The window is divided into several areas:\n\
         • Sidebar (left) — file tree for browsing and opening project files\n\
         • Editor (centre) — tabbed text editor with syntax highlighting\n\
         • Canvas (centre, optional) — live preview of the .ui file you're editing\n\
         • Toolbox (left, optional) — widget palette and property inspector for .ui design\n\
         • Preview pane (right, optional) — rendered view of HTML/CSS files\n\
         • Minimap (right edge, optional) — scrollable overview of the current file\n\
         • Output panel (bottom) — run output, build results, and error messages\n\n\
         Use the View menu or the shortcuts below to show and hide each area."
    );

    // ── Menu vs Toolbar ──────────────────────────────────────────────────────
    append_section(&content, "Menu vs Toolbar");
    append_text(&content,
        "The toolbar gives you one-click access to the most common actions:\n\
         New Project, Open Project, Save All, Undo/Redo, Generate Handlers,\n\
         Template Library, Run, Build, Build Install, Stop, Dark Mode, and Help.\n\n\
         The menu bar covers everything — including less-frequent actions such as\n\
         New Tab, Open File, Save As, Find, Go to Line, view toggles, layouts,\n\
         and all AI tools. If a button is on the toolbar it is also in the menu;\n\
         the toolbar is purely a shortcut, not a separate feature set."
    );

    // ── Opening Files & Projects ─────────────────────────────────────────────
    append_section(&content, "Opening Files & Projects");
    append_text(&content,
        "• File → Open  or  Ctrl+O — open a single file via dialog\n\
         • File → Open Project — choose a project folder; its files appear in the sidebar\n\
         • Double-click any file in the sidebar to open it\n\
         • Pass file paths on the command line:  rui file.ui file.rs\n\
         • Drag files from a file manager and drop onto the editor window\n\
         • Recent Files and Recent Projects submenus keep track of what you've opened\n\n\
         If a file is already open in a tab, switching to it reuses the existing tab."
    );

    // ── Tabs ─────────────────────────────────────────────────────────────────
    append_section(&content, "Tabs");
    append_text(&content,
        "• Ctrl+N — new empty tab\n\
         • Ctrl+W — close current tab\n\
         • Click the × on a tab label to close it\n\
         • Tabs can be reordered by dragging\n\
         • A dot (●) in the tab label means the file has unsaved changes"
    );

    // ── Editing ──────────────────────────────────────────────────────────────
    append_section(&content, "Editing");
    append_shortcuts(&content, &[
        ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
        ("Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste"),
        ("Ctrl+A",                  "Select all"),
        ("Tab / Shift+Tab",         "Indent / unindent selection"),
        ("Home",                    "Jump to first non-whitespace on line"),
        ("Ctrl+F",                  "Find"),
        ("Ctrl+H",                  "Find & Replace"),
        ("Ctrl+G",                  "Go to Line"),
    ]);
    append_text(&content,
        "In Designer view, Undo/Redo operates on the .ui XML snapshot history\n\
         rather than the text buffer directly."
    );

    // ── Find & Replace ───────────────────────────────────────────────────────
    append_section(&content, "Find & Replace");
    append_text(&content,
        "Press Ctrl+F to open the find bar. The bar has two rows:\n\
         • Top row: search entry, ◀ ▶ navigation, Aa (case-sensitive), .* (regex)\n\
         • Bottom row: replace entry, Replace (current match), Replace All\n\n\
         Press Enter or ▶ to jump to the next match. The match count updates as you type.\n\
         Press Escape or click × to close the bar."
    );

    // ── Saving ───────────────────────────────────────────────────────────────
    append_section(&content, "Saving");
    append_shortcuts(&content, &[
        ("Ctrl+S",       "Save (prompts for path if the file has none yet)"),
        ("Ctrl+Shift+S", "Save As…"),
    ]);
    append_text(&content,
        "File → Save All saves every tab that has unsaved changes at once.\n\
         Enable autosave in rui.toml (autosave = true) to save automatically\n\
         whenever the editor loses focus."
    );

    // ── Building & Running ───────────────────────────────────────────────────
    append_section(&content, "Building & Running");
    append_shortcuts(&content, &[
        ("F5",           "Run current file"),
        ("Ctrl+Shift+B", "Build project (cargo build --release for Rust)"),
        ("Shift+F5",     "Stop the running process"),
        ("Ctrl+J",       "Toggle output panel"),
    ]);
    append_text(&content,
        "Supported languages and runners:\n\
         • Rust (.rs) — cargo run (auto-discovers Cargo.toml walking up the tree)\n\
         • Python (.py) — python3\n\
         • JavaScript (.js / .mjs) — node\n\
         • TypeScript (.ts) — node --experimental-strip-types (or ts-node)\n\
         • Shell (.sh) — bash\n\
         • HTML/CSS — opens in your default browser via xdg-open\n\n\
         Build runs a pre-flight cargo check first; if any errors are found the\n\
         release build is skipped and the errors are shown in the output panel.\n\n\
         Build Install (Run menu / toolbar) does the same pre-flight, then runs\n\
         cargo build --release and generates a .desktop launcher and copies the\n\
         app icon to the release directory so the app can be installed system-wide.\n\n\
         Output appears in the Run tab of the output panel.\n\
         Errors are shown in red; a green exit message means success."
    );

    // ── Designing .ui Files ──────────────────────────────────────────────────
    append_section(&content, "Designing .ui Files");
    append_text(&content,
        "GTK4 .ui files are XML that describes a window layout. Rui shows a live\n\
         canvas preview alongside the editor so you can see changes as you type.\n\n\
         Canvas interactions:\n\
         • Single-click a widget on the canvas to select it (highlights in the XML)\n\
         • Double-click a widget to jump to its definition in the editor\n\
         • The canvas refreshes ~500 ms after the last keystroke\n\n\
         Toolbox (Ctrl+Shift+T):\n\
         • Widget Palette — browse GTK4 widgets by category and click to insert them\n\
         • Property Inspector — edit the properties of the selected XML element\n\
         • Widget selector dropdown — quickly navigate to any widget by ID\n\n\
         Run → Generate All Handlers scans the .ui file for signal attributes and\n\
         writes matching Rust handler stubs into the project source so you can\n\
         connect them without writing boilerplate by hand.\n\n\
         Use Ctrl+1 (Code View) when you want to focus on writing Rust, and\n\
         Ctrl+2 (Designer View) when you want the canvas and palette front and centre."
    );
    append_markup(&content,
        "For a full reference of available GTK4 widgets and their properties, see:\n\
         <a href=\"https://docs.gtk.org/gtk4/\">docs.gtk.org/gtk4</a>"
    );

    // ── AI Tools ─────────────────────────────────────────────────────────────
    append_section(&content, "AI Tools");
    append_shortcuts(&content, &[
        ("Ctrl+Alt+A", "Open AI Chat panel"),
        ("Ctrl+Alt+C", "Copy current file to clipboard with AI context"),
        ("Ctrl+Alt+D", "Apply an AI-generated diff to the current file"),
    ]);
    append_text(&content,
        "The AI menu provides three panels (shown in the right pane):\n\
         • Claude Code — a full Claude Code terminal session inside Rui\n\
         • AI Chat — a web-based chat interface\n\
         • API Chat — a direct Anthropic API chat for quick questions\n\n\
         AI → Copy Selection for AI puts the highlighted text on the clipboard\n\
         with enough context for an AI assistant to understand where it came from.\n\
         AI → Apply Diff pastes an AI-suggested diff and applies it to the file."
    );

    // ── View & Layouts ───────────────────────────────────────────────────────
    append_section(&content, "View & Layouts");
    append_shortcuts(&content, &[
        ("Ctrl+1",       "Code View — full sidebar, editor + minimap, no canvas"),
        ("Ctrl+2",       "Designer View — canvas + toolbox, narrow sidebar"),
        ("Ctrl+B",       "Toggle sidebar"),
        ("Ctrl+J",       "Toggle output panel"),
        ("Ctrl+Shift+U", "Toggle .ui canvas preview"),
        ("Ctrl+Shift+T", "Toggle toolbox (widget palette + inspector)"),
        ("Ctrl+Shift+P", "Toggle HTML/CSS preview pane"),
        ("Ctrl+M",       "Toggle minimap"),
        ("Ctrl+Shift+D", "Toggle dark mode"),
    ]);

    // ── Syntax Highlighting & Themes ─────────────────────────────────────────
    append_section(&content, "Syntax Highlighting & Themes");
    append_text(&content,
        "Syntax highlighting is provided by GtkSourceView 5 and is detected\n\
         automatically from the file extension (.ui files are treated as XML).\n\
         The default colour scheme is rui-theme (Catppuccin Mocha inspired).\n\n\
         To change the scheme, set color_scheme in rui.toml under [editor].\n\
         Available built-in schemes: classic, oblivion, solarized-dark,\n\
         solarized-light, kate, tango, cobalt, and rui-theme."
    );

    // ── Configuration ────────────────────────────────────────────────────────
    append_section(&content, "Configuration");
    append_text(&content,
        "All settings live in ~/.config/rui/rui.toml under [editor]:\n\
         • font — Pango font string, e.g. \"JetBrains Mono 13\"\n\
         • tab_width — spaces per tab stop (default 4)\n\
         • insert_spaces — use spaces instead of tabs (default true)\n\
         • show_line_numbers — gutter line numbers (default true)\n\
         • highlight_current_line — highlight cursor line (default true)\n\
         • word_wrap — soft word wrap (default false)\n\
         • color_scheme — GtkSourceView scheme ID (default \"rui-theme\")\n\
         • show_sidebar — show file tree on startup (default true)\n\
         • show_output — show output panel on startup (default false)\n\
         • show_preview — show preview pane on startup (default true)\n\
         • default_dir — directory to open on startup (default: home)\n\
         • autosave — save on focus loss (default false)"
    );

    scroll.set_child(Some(&content));
    root.append(&scroll);

    root.append(&Separator::new(Orientation::Horizontal));
    let btn_row = GtkBox::new(Orientation::Horizontal, 0);
    btn_row.set_halign(Align::End);
    btn_row.set_margin_start(16);
    btn_row.set_margin_end(16);
    btn_row.set_margin_top(8);
    btn_row.set_margin_bottom(8);

    let close_btn = Button::with_label("Close");
    close_btn.add_css_class("suggested-action");
    let win_c = win.clone();
    close_btn.connect_clicked(move |_| win_c.close());

    btn_row.append(&close_btn);
    root.append(&btn_row);

    win.set_child(Some(&root));

    let key_ctrl = gtk4::EventControllerKey::new();
    let win_c = win.clone();
    key_ctrl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            win_c.close();
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    win.add_controller(key_ctrl);

    win.present();
}

fn append_section(parent: &GtkBox, title: &str) {
    let lbl = Label::new(None);
    lbl.set_markup(&format!("<b>{}</b>", title));
    lbl.set_halign(Align::Start);
    lbl.set_margin_top(16);
    lbl.set_margin_bottom(4);
    parent.append(&lbl);

    let sep = Separator::new(Orientation::Horizontal);
    sep.set_margin_bottom(6);
    parent.append(&sep);
}

fn append_text(parent: &GtkBox, text: &str) {
    let lbl = Label::new(Some(text));
    lbl.set_halign(Align::Start);
    lbl.set_wrap(true);
    lbl.set_xalign(0.0);
    lbl.set_margin_bottom(4);
    parent.append(&lbl);
}

fn append_markup(parent: &GtkBox, markup: &str) {
    let lbl = Label::new(None);
    lbl.set_markup(markup);
    lbl.set_halign(Align::Start);
    lbl.set_wrap(true);
    lbl.set_xalign(0.0);
    lbl.set_margin_top(4);
    lbl.set_margin_bottom(4);
    parent.append(&lbl);
}

fn append_shortcuts(parent: &GtkBox, rows: &[(&str, &str)]) {
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(24);
    grid.set_row_spacing(2);
    grid.set_margin_bottom(4);

    for (i, (key, desc)) in rows.iter().enumerate() {
        let key_lbl = Label::new(None);
        key_lbl.set_markup(&format!("<tt>{}</tt>", key));
        key_lbl.set_halign(Align::Start);
        key_lbl.set_valign(Align::Start);

        let desc_lbl = Label::new(Some(desc));
        desc_lbl.set_halign(Align::Start);
        desc_lbl.set_wrap(true);
        desc_lbl.set_xalign(0.0);

        grid.attach(&key_lbl,  0, i as i32, 1, 1);
        grid.attach(&desc_lbl, 1, i as i32, 1, 1);
    }

    parent.append(&grid);
}
