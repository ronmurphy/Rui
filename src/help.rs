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
        .default_width(620)
        .default_height(560)
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

    append_section(&content, "Layout");
    append_text(&content,
        "The editor is divided into three areas:\n\
         • Sidebar (left) — file tree for browsing and opening files\n\
         • Editor (centre) — tabbed text editor with syntax highlighting\n\
         • Output panel (bottom) — run output, problems, and build results\n\n\
         An optional live Preview pane (right) shows HTML/CSS files or .ui widget previews."
    );

    append_section(&content, "Opening Files");
    append_text(&content,
        "• File → Open  or  Ctrl+O  — open a file via dialog\n\
         • Double-click any file in the sidebar\n\
         • Pass file paths on the command line:  rui file.ui file.rs\n\
         • Drag files from a file manager and drop onto the editor window\n\n\
         If a file is already open in a tab, switching to it re-uses the existing tab."
    );

    append_section(&content, "Tabs");
    append_text(&content,
        "• Ctrl+N — new empty tab\n\
         • Ctrl+W — close current tab\n\
         • Click the × button on a tab label to close it\n\
         • Tabs can be reordered by dragging\n\
         • A dot (●) in the tab label means the file has unsaved changes"
    );

    append_section(&content, "Editing");
    append_shortcuts(&content, &[
        ("Ctrl+Z / Ctrl+Shift+Z",  "Undo / Redo"),
        ("Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste"),
        ("Ctrl+A",                 "Select all"),
        ("Tab / Shift+Tab",        "Indent / unindent selection"),
        ("Home",                   "Jump to first non-whitespace on line"),
        ("Ctrl+F",                 "Find"),
        ("Ctrl+H",                 "Find & Replace"),
        ("Ctrl+G",                 "Go to Line"),
    ]);

    append_section(&content, "Find & Replace");
    append_text(&content,
        "Press Ctrl+F to open the find bar. The bar has two rows:\n\
         • Top row: search entry, ◀ ▶ navigation, Aa (case sensitive), .* (regex)\n\
         • Bottom row: replace entry, Replace (current match), Replace All\n\n\
         Press Enter or ▶ to jump to the next match. The match count updates as you type.\n\
         Press Escape or click the × to close the bar."
    );

    append_section(&content, "Saving");
    append_shortcuts(&content, &[
        ("Ctrl+S",        "Save (triggers Save As if file has no path yet)"),
        ("Ctrl+Shift+S",  "Save As…"),
    ]);
    append_text(&content,
        "Enable autosave in rui.toml under [editor] (autosave = true) to automatically\n\
         save the current file whenever the editor loses focus."
    );

    append_section(&content, "Running Code");
    append_shortcuts(&content, &[
        ("F5",            "Run current file"),
        ("Ctrl+Shift+B",  "Build (cargo build for Rust, same as Run for others)"),
        ("Shift+F5",      "Stop running process"),
        ("Ctrl+J",        "Toggle output panel"),
    ]);
    append_text(&content,
        "Supported languages and runners:\n\
         • Python (.py) — python3\n\
         • JavaScript (.js) — node\n\
         • TypeScript (.ts) — node --experimental-strip-types\n\
         • Rust (.rs) — cargo run (auto-discovers Cargo.toml walking up)\n\
         • Shell (.sh) — bash\n\
         • HTML (.html) — opens in default browser via xdg-open\n\n\
         Output appears in the Run tab of the output panel. \
         Errors are highlighted in red, success messages in green."
    );

    append_section(&content, "View");
    append_shortcuts(&content, &[
        ("Ctrl+B",        "Toggle sidebar"),
        ("Ctrl+J",        "Toggle output panel"),
        ("Ctrl+Shift+U",  "Toggle .ui canvas preview"),
        ("Ctrl+Shift+T",  "Toggle toolbox (widgets + inspector)"),
        ("Ctrl+Shift+P",  "Toggle HTML/CSS preview pane"),
    ]);

    append_section(&content, "Layouts");
    append_shortcuts(&content, &[
        ("Ctrl+1",        "Code View — code focused, no canvas/palette"),
        ("Ctrl+2",        "Designer View — canvas + palette, narrow sidebar"),
    ]);

    append_section(&content, "Syntax Highlighting & Themes");
    append_text(&content,
        "Syntax highlighting is provided by GtkSourceView 5 and automatically detects\n\
         language from the file extension (.ui files are treated as XML).\n\
         The default colour scheme is rui-theme (Catppuccin Mocha inspired).\n\n\
         To change the colour scheme, set color_scheme in rui.toml [editor].\n\
         Available built-in schemes: classic, oblivion, solarized-dark, solarized-light,\n\
         kate, tango, cobalt, and rui-theme."
    );

    append_section(&content, "Configuration");
    append_text(&content,
        "All editor settings live in ~/.config/rui/rui.toml under [editor]:\n\
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
