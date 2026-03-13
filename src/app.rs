use sourceview5::{prelude::*, Map};
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CheckButton,
    FileDialog, Frame, Label, ListBox, Notebook as GtkNotebook, Orientation,
    Paned, ScrolledWindow, SelectionMode, SpinButton, Window,
};
use crate::config::EditorConfig;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::diff_tool;
use crate::diagnostics;
use crate::find::FindBar;
use crate::goto;
use crate::help;
use crate::menubar;
use crate::canvas::Canvas;
use crate::claude_code::ClaudeCodePanel;
use crate::ai_chat_panel::AiChatPanel;
use crate::outline::OutlinePanel;
use crate::toolbox::Toolbox;
use crate::node_view::NodeView;

#[cfg(feature = "preview")]
use crate::ai_panel;
use crate::notebook::NotebookManager;
use crate::output::OutputPanel;
use crate::runner::RunManager;
use crate::sidebar::FileTree;
use crate::statusbar::StatusBar;


#[cfg(feature = "preview")]
use crate::preview::PreviewPane;

/// Central application state shared across all closures.
struct AppState {
    window:    ApplicationWindow,
    notebook:  NotebookManager,
    sidebar:   FileTree,
    output:    OutputPanel,
    statusbar: StatusBar,
    runner:    RunManager,
    find_bar:  FindBar,
    cfg:       EditorConfig,
    canvas:    Canvas,
    toolbox:   Toolbox,
    outline:   OutlinePanel,
    node_view: NodeView,
    /// Current layout: "code" or "designer"
    layout:    String,
    /// The project directory (set by Open Project / New Project).
    /// Used by Run/Build to find the correct Cargo.toml.
    project_dir: Option<PathBuf>,
    /// Designer undo/redo history (XML snapshots on disk).
    history: Rc<RefCell<crate::history::DesignerHistory>>,
    /// True while an undo/redo operation is applying XML to the buffer,
    /// so the buffer-changed hook skips taking a new snapshot.
    history_applying: Rc<Cell<bool>>,
    /// Most-recently-used file/project list (persisted to mru.json).
    mru: crate::mru::Mru,
    /// Last diagnostics from cargo check / clippy / LSP (for inline marks).
    last_diagnostics: Rc<RefCell<Vec<crate::diagnostics::Diagnostic>>>,
    /// Live LSP connection to rust-analyzer (None if not a Rust project or RA missing).
    lsp: Option<crate::lsp_client::LspClient>,
    /// Dynamic gio::Menu backing the File → Recent Files submenu.
    recent_files_menu: gtk4::gio::Menu,
    /// Dynamic gio::Menu backing the File → Recent Projects submenu.
    recent_projects_menu: gtk4::gio::Menu,

    #[cfg(feature = "preview")]
    preview: PreviewPane,

    #[cfg(feature = "preview")]
    ai_sidebar: crate::ai_panel::AiSidebar,

    claude_panel: ClaudeCodePanel,
    ai_chat_panel: AiChatPanel,
    chat_history: crate::chat_history::ChatHistoryPanel,

    main_paned: Paned,
    vert_paned: Paned,
    left_nb:   GtkNotebook,
    center_nb: GtkNotebook,
    right_nb:  GtkNotebook,
    right_paned: Paned,
    ai_stack: gtk4::Stack,
    minimap: Map,
}

/// Show the "New .ui File" layout dialog.
/// Calls `on_confirm(xml)` with the ready-to-use .ui XML when the user clicks Create.
fn show_layout_dialog(
    parent: &ApplicationWindow,
    on_confirm: impl Fn(String) + 'static,
) {
    let dialog = Window::builder()
        .title("New .ui File")
        .modal(true)
        .transient_for(parent)
        .destroy_with_parent(true)
        .resizable(false)
        .default_width(380)
        .build();

    let outer = GtkBox::new(Orientation::Vertical, 16);
    outer.set_margin_start(20);
    outer.set_margin_end(20);
    outer.set_margin_top(20);
    outer.set_margin_bottom(20);

    // ── Root layout choice ──
    let box_radio  = CheckButton::with_label("GtkBox  (simple vertical list)");
    let grid_radio = CheckButton::with_label("GtkGrid  (grid-based layout)");
    grid_radio.set_group(Some(&box_radio));
    grid_radio.set_active(true);
    outer.append(&box_radio);
    outer.append(&grid_radio);

    // ── Grid dimensions ──
    let dims = GtkBox::new(Orientation::Horizontal, 8);
    dims.set_margin_top(4);

    let rows_label = Label::new(Some("Rows:"));
    let rows_spin  = SpinButton::with_range(1.0, 20.0, 1.0);
    rows_spin.set_value(7.0);
    rows_spin.set_width_chars(3);

    let cols_label = Label::new(Some("Columns:"));
    let cols_spin  = SpinButton::with_range(1.0, 20.0, 1.0);
    cols_spin.set_value(8.0);
    cols_spin.set_width_chars(3);

    dims.append(&rows_label);
    dims.append(&rows_spin);
    dims.append(&cols_label);
    dims.append(&cols_spin);
    outer.append(&dims);

    let hint = Label::new(Some("Cells are numbered from 0 — 4 columns gives you columns 0 to 3."));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    outer.append(&hint);

    // ── Template picker (grid only) ──
    let tmpl_frame = Frame::new(Some("Template"));
    tmpl_frame.set_margin_top(4);

    let tmpl_list = ListBox::new();
    tmpl_list.set_selection_mode(SelectionMode::Single);

    for tmpl in crate::templates::TEMPLATES {
        let row_box = GtkBox::new(Orientation::Vertical, 2);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        let name_lbl = Label::new(Some(tmpl.label));
        name_lbl.set_xalign(0.0);
        let desc_lbl = Label::new(Some(tmpl.description));
        desc_lbl.set_xalign(0.0);
        desc_lbl.add_css_class("dim-label");
        desc_lbl.set_wrap(true);
        row_box.append(&name_lbl);
        row_box.append(&desc_lbl);
        // Store template id in widget name so we can retrieve it on confirm
        row_box.set_widget_name(tmpl.id);
        tmpl_list.append(&row_box);
    }
    // Select first row (Blank) by default
    if let Some(first_row) = tmpl_list.row_at_index(0) {
        tmpl_list.select_row(Some(&first_row));
    }

    let tmpl_scroll = ScrolledWindow::new();
    tmpl_scroll.set_min_content_height(160);
    tmpl_scroll.set_child(Some(&tmpl_list));
    tmpl_frame.set_child(Some(&tmpl_scroll));
    outer.append(&tmpl_frame);

    // Toggle dims + template sensitivity when radio changes
    {
        let dims_ref   = dims.clone();
        let tmpl_ref   = tmpl_frame.clone();
        grid_radio.connect_toggled(move |btn| {
            dims_ref.set_sensitive(btn.is_active());
            tmpl_ref.set_sensitive(btn.is_active());
        });
    }

    // ── Buttons ──
    let btn_row = GtkBox::new(Orientation::Horizontal, 8);
    btn_row.set_halign(Align::End);
    btn_row.set_margin_top(8);

    let cancel_btn = Button::with_label("Cancel");
    let create_btn = Button::with_label("Create");
    create_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&create_btn);
    outer.append(&btn_row);

    dialog.set_child(Some(&outer));

    {
        let d = dialog.clone();
        cancel_btn.connect_clicked(move |_| d.close());
    }
    {
        let d = dialog.clone();
        create_btn.connect_clicked(move |_| {
            let use_grid = grid_radio.is_active();
            let rows = rows_spin.value() as i32;
            let cols = cols_spin.value() as i32;

            let xml = if use_grid {
                // Find selected template id from the list
                let tmpl_id = tmpl_list
                    .selected_row()
                    .and_then(|row| row.child())
                    .map(|w| w.widget_name().to_string())
                    .unwrap_or_else(|| "blank".into());
                crate::templates::apply(&tmpl_id, rows, cols)
            } else {
                crate::codegen::make_box_template()
            };
            d.close();
            on_confirm(xml);
        });
    }

    dialog.present();
}

/// If `tab` holds a .ui file, parse the XML and report errors to the output panel.
/// Called after every save so broken XML is caught immediately.
fn validate_ui_tab(tab: &crate::tab::EditorTab, output: &crate::output::OutputPanel) {
    let path = match tab.path() {
        Some(p) if Canvas::is_ui_file(&p) => p,
        _ => return,
    };
    let (start, end) = tab.buffer().bounds();
    let text = tab.buffer().text(&start, &end, false).to_string();
    match roxmltree::Document::parse(&text) {
        Ok(_) => {} // valid — no noise
        Err(e) => {
            output.append_run_error(&format!(
                "⚠ XML error in {}: {}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                e
            ));
            output.show_panel();
        }
    }
}

/// Short display label for a path: "parentdir/filename".
fn path_display_label(path: &std::path::Path) -> String {
    let name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
        return format!("{}/{}", parent.to_string_lossy(), name);
    }
    name
}

/// Rebuild both MRU sub-menus from the current `Mru` state.
fn rebuild_mru_menus(
    mru: &crate::mru::Mru,
    files_menu: &gtk4::gio::Menu,
    projects_menu: &gtk4::gio::Menu,
) {
    use gtk4::glib::prelude::ToVariant;

    files_menu.remove_all();
    for path in &mru.files {
        let label = path_display_label(path);
        let item = gtk4::gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(
            Some("app.open-recent-file"),
            Some(&path.to_string_lossy().to_string().to_variant()),
        );
        files_menu.append_item(&item);
    }
    if mru.files.is_empty() {
        files_menu.append(Some("(no recent files)"), None);
    }

    projects_menu.remove_all();
    for path in &mru.projects {
        let label = path_display_label(path);
        let item = gtk4::gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(
            Some("app.open-recent-project"),
            Some(&path.to_string_lossy().to_string().to_variant()),
        );
        projects_menu.append_item(&item);
    }
    if mru.projects.is_empty() {
        projects_menu.append(Some("(no recent projects)"), None);
    }
}

pub fn build_ui(app: &Application, open_paths: Vec<PathBuf>) {
    let cfg = crate::config::load();

    // Apply persisted dark mode preference
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(cfg.dark_mode);
    }

    // In-memory dark mode state — source of truth for toggling
    let dark_state = Rc::new(Cell::new(cfg.dark_mode));

    let notebook   = NotebookManager::new();
    let sidebar    = FileTree::new();
    let output     = OutputPanel::new();
    let statusbar  = StatusBar::new();
    let runner     = RunManager::new();
    let find_bar   = FindBar::new();
    let canvas     = Canvas::new();
    canvas.init_merge_toolbar();
    canvas.init_zoom();
    {
        let out = output.clone();
        canvas.on_xml_error(move |msg| {
            out.append_run_error(&format!("[Canvas] {}", msg));
            out.show_panel();
        });
    }
    let toolbox    = Toolbox::new();
    let outline    = OutlinePanel::new();

    #[cfg(feature = "preview")]
    let preview = PreviewPane::new();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Rui")
        .default_width(1200)
        .default_height(800)
        .build();

    let (menubar_model, recent_files_menu, recent_projects_menu) = menubar::build(app);

    // Sync initial toggle-dark action state from persisted config
    if let Some(action) = app.lookup_action("toggle-dark") {
        if let Some(sa) = action.downcast_ref::<gtk4::gio::SimpleAction>() {
            use gtk4::glib::variant::ToVariant;
            sa.set_state(&cfg.dark_mode.to_variant());
        }
    }

    // ── Header Bar (CSD) ─────────────────────────────────────────
    let header_bar = gtk4::HeaderBar::new();
    window.set_titlebar(Some(&header_bar));

    let menu_btn = gtk4::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_menu_model(Some(&menubar_model));
    header_bar.pack_start(&menu_btn);

    // View Switcher (Design / Code)
    let view_switcher = GtkBox::new(Orientation::Horizontal, 0);
    view_switcher.add_css_class("linked");

    let btn_design = gtk4::ToggleButton::with_label("Design");
    let btn_code   = gtk4::ToggleButton::with_label("Code");
    let btn_nodes  = gtk4::ToggleButton::with_label("Nodes");
    btn_design.set_active(true);
    btn_code.set_group(Some(&btn_design));
    btn_nodes.set_group(Some(&btn_design));

    view_switcher.append(&btn_design);
    view_switcher.append(&btn_code);
    view_switcher.append(&btn_nodes);
    header_bar.set_title_widget(Some(&view_switcher));

    let minimap = Map::new();
    minimap.set_size_request(100, -1);
    minimap.set_visible(false);

    let editor_body = GtkBox::new(Orientation::Horizontal, 0);
    editor_body.set_hexpand(true);
    editor_body.set_vexpand(true);
    editor_body.append(&notebook.widget);
    editor_body.append(&minimap);

    let editor_col = GtkBox::new(Orientation::Vertical, 0);
    editor_col.set_hexpand(true);
    editor_col.set_vexpand(true);
    editor_col.append(&editor_body);
    editor_col.append(&find_bar.widget);

    // ── Center notebook: Design (page 0) | Code (page 1) | Nodes (page 2) ──
    let node_view = NodeView::new();

    let center_nb = GtkNotebook::new();
    center_nb.set_show_border(false);
    center_nb.set_show_tabs(false);
    center_nb.set_hexpand(true);
    center_nb.set_vexpand(true);
    canvas.widget.set_hexpand(true);
    canvas.widget.set_vexpand(true);
    node_view.widget.set_hexpand(true);
    node_view.widget.set_vexpand(true);
    center_nb.append_page(&canvas.widget,      Some(&Label::new(Some("  Design  "))));
    center_nb.append_page(&editor_col,         Some(&Label::new(Some("  Code  "))));
    center_nb.append_page(&node_view.widget,   Some(&Label::new(Some("  Nodes  "))));

    // Re-render canvas when switching to Design; refresh node graph for Nodes.
    {
        let canvas_ref    = canvas.clone();
        let node_view_ref = node_view.clone();
        center_nb.connect_switch_page(move |_, _, page| {
            if page == 0 { canvas_ref.render_from_buffer(); }
            if page == 2 { node_view_ref.refresh(); }
        });
    }

    // Each toggle button only acts when it becomes *active* (radio semantics).
    {
        let cnb = center_nb.clone();
        btn_design.connect_toggled(move |btn| {
            if btn.is_active() { cnb.set_current_page(Some(0)); }
        });
    }
    {
        let cnb = center_nb.clone();
        btn_code.connect_toggled(move |btn| {
            if btn.is_active() { cnb.set_current_page(Some(1)); }
        });
    }
    {
        let cnb = center_nb.clone();
        btn_nodes.connect_toggled(move |btn| {
            if btn.is_active() { cnb.set_current_page(Some(2)); }
        });
    }

    // ── Left notebook: Tree | Widgets | Files ────────
    let left_nb = GtkNotebook::new();
    left_nb.set_show_border(false);
    left_nb.set_width_request(230);
    sidebar.widget.set_visible(true);
    toolbox.widget.set_visible(true);
    outline.widget.set_visible(true);
    left_nb.append_page(&outline.widget,  Some(&Label::new(Some("  Tree  "))));
    left_nb.append_page(&toolbox.widget,  Some(&Label::new(Some("  Widgets  "))));
    left_nb.append_page(&sidebar.widget,  Some(&Label::new(Some("  Files  "))));

    // Default to Designer layout
    left_nb.set_current_page(Some(1));   // start on Widgets tab
    center_nb.set_current_page(Some(0)); // start on Design tab
    if !cfg.show_sidebar {
        left_nb.set_visible(false);
    }

    #[cfg(feature = "preview")]
    if !cfg.show_preview {
        preview.widget.set_visible(false);
    }

    // ── AI sidebar (right-hand panel, hidden until toggled) ────────
    #[cfg(feature = "preview")]
    let ai_sidebar = crate::ai_panel::AiSidebar::new();

    // ── Claude Code panel (native CLI chat, always available) ─────
    let claude_panel = ClaudeCodePanel::new();

    // ── API Chat panel (multi-provider, key-based, no CLI required) ──
    let ai_chat_panel = AiChatPanel::new();

    // ── Master Toolbar ────────
    // Helper to create tool buttons
    let make_tool_btn = |icon: &str, tooltip: &str, action: &str| -> gtk4::Button {
        let btn = gtk4::Button::builder()
            .label(icon)
            .tooltip_text(tooltip)
            .action_name(action)
            .build();
        btn.add_css_class("nf");
        btn.add_css_class("flat");
        btn.add_css_class("toolbar-icon");
        btn
    };

    let master_toolbar = GtkBox::new(Orientation::Horizontal, 2);
    master_toolbar.add_css_class("toolbar");
    master_toolbar.set_margin_top(2);
    master_toolbar.set_margin_bottom(2);
    master_toolbar.set_margin_start(4);
    master_toolbar.set_margin_end(4);
    master_toolbar.set_halign(gtk4::Align::Center);

    // ── Project combo button (MenuButton + Popover) ───────────────────────────
    // Reusable pattern: one toolbar icon → popover with labelled action rows.
    let make_popup_entry = |icon: &str, label: &str, action: &str, pop: &gtk4::Popover| -> gtk4::Button {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_margin_start(4);
        row.set_margin_end(8);
        row.set_margin_top(2);
        row.set_margin_bottom(2);
        let icon_lbl = gtk4::Label::new(Some(icon));
        icon_lbl.add_css_class("nf");
        let text_lbl = gtk4::Label::new(Some(label));
        text_lbl.set_halign(gtk4::Align::Start);
        text_lbl.set_hexpand(true);
        row.append(&icon_lbl);
        row.append(&text_lbl);
        let btn = gtk4::Button::builder().build();
        btn.set_child(Some(&row));
        btn.add_css_class("toolbar-popup-entry");
        let pop2 = pop.clone();
        let act = action.to_string();
        btn.connect_clicked(move |b| {
            pop2.popdown();
            let _ = b.activate_action(&act, None);
        });
        btn
    };

    let proj_popover = gtk4::Popover::new();
    let proj_box = GtkBox::new(Orientation::Vertical, 2);
    proj_box.add_css_class("toolbar-popup");
    proj_box.set_margin_top(4);
    proj_box.set_margin_bottom(4);
    proj_box.append(&make_popup_entry("\u{f067}", "New Project…",  "app.new-project",  &proj_popover));
    proj_box.append(&make_popup_entry("\u{f115}", "Open Project…", "app.open-project", &proj_popover));
    proj_box.append(&gtk4::Separator::new(Orientation::Horizontal));
    proj_box.append(&make_popup_entry("\u{f0c7}", "Save All",      "app.save-all",     &proj_popover));
    proj_popover.set_child(Some(&proj_box));

    let proj_btn = gtk4::MenuButton::builder()
        .label("\u{f07b}")          // nf-fa-folder
        .tooltip_text("Project…")
        .popover(&proj_popover)
        .build();
    proj_btn.add_css_class("nf");
    proj_btn.add_css_class("flat");
    proj_btn.add_css_class("toolbar-icon");
    master_toolbar.append(&proj_btn);
    // ─────────────────────────────────────────────────────────────────────────

    let sep1 = gtk4::Separator::new(Orientation::Vertical);
    sep1.set_margin_start(4); sep1.set_margin_end(4);
    master_toolbar.append(&sep1);
    
    master_toolbar.append(&make_tool_btn("\u{f0e2}", "Undo", "app.designer-undo"));
    master_toolbar.append(&make_tool_btn("\u{f01e}", "Redo", "app.designer-redo"));
    
    let sep2 = gtk4::Separator::new(Orientation::Vertical);
    sep2.set_margin_start(4); sep2.set_margin_end(4);
    master_toolbar.append(&sep2);
    
    master_toolbar.append(&make_tool_btn("\u{f013}", "Generate Handlers", "app.generate-handlers"));
    master_toolbar.append(&make_tool_btn("\u{f02d}", "Template Library", "app.template-library"));
    
    let sep3 = gtk4::Separator::new(Orientation::Vertical);
    sep3.set_margin_start(4); sep3.set_margin_end(4);
    master_toolbar.append(&sep3);
    
    // ── Run combo button ──────────────────────────────────────────────────────
    let run_popover = gtk4::Popover::new();
    let run_box = GtkBox::new(Orientation::Vertical, 2);
    run_box.add_css_class("toolbar-popup");
    run_box.set_margin_top(4);
    run_box.set_margin_bottom(4);
    run_box.append(&make_popup_entry("\u{f04b}", "Run",           "app.run",           &run_popover));
    run_box.append(&make_popup_entry("\u{f085}", "Build",         "app.build",         &run_popover));
    run_box.append(&make_popup_entry("\u{f1c0}", "Build Install", "app.build-install", &run_popover));
    run_box.append(&gtk4::Separator::new(Orientation::Horizontal));
    run_box.append(&make_popup_entry("\u{f00c}", "Check",         "app.check",         &run_popover));
    run_box.append(&make_popup_entry("\u{f0a9}", "Clippy",        "app.clippy",        &run_popover));
    run_box.append(&make_popup_entry("\u{f121}", "Format File",   "app.format-file",   &run_popover));
    run_box.append(&gtk4::Separator::new(Orientation::Horizontal));
    run_box.append(&make_popup_entry("\u{f04d}", "Stop",          "app.stop",          &run_popover));
    run_popover.set_child(Some(&run_box));

    let run_btn = gtk4::MenuButton::builder()
        .label("\u{f04b}")
        .tooltip_text("Run…")
        .popover(&run_popover)
        .build();
    run_btn.add_css_class("nf");
    run_btn.add_css_class("flat");
    run_btn.add_css_class("toolbar-icon");
    master_toolbar.append(&run_btn);
    // ─────────────────────────────────────────────────────────────────────────

    let sep4 = gtk4::Separator::new(Orientation::Vertical);
    sep4.set_margin_start(4); sep4.set_margin_end(4);
    master_toolbar.append(&sep4);
    
    // Dark mode button — kept separately so we can swap the icon on toggle
    let dark_btn = gtk4::Button::builder()
        .label(if cfg.dark_mode { "\u{f185}" } else { "\u{f186}" })
        .tooltip_text(if cfg.dark_mode { "Switch to Light Mode" } else { "Switch to Dark Mode" })
        .action_name("app.toggle-dark")
        .build();
    dark_btn.add_css_class("nf");
    dark_btn.add_css_class("flat");
    dark_btn.add_css_class("toolbar-icon");
    master_toolbar.append(&dark_btn);
    master_toolbar.append(&make_tool_btn("\u{f29c}", "Help", "app.help"));

    // Wrap center_nb in center_area (with Master Toolbar above)
    let center_area = GtkBox::new(Orientation::Vertical, 0);
    center_area.set_hexpand(true);
    center_area.set_vexpand(true);
    center_area.append(&master_toolbar);
    center_area.append(&center_nb);

    // ── Right notebook: Properties | AI Chat ────────
    let right_nb = GtkNotebook::new();
    right_nb.set_show_border(false);
    right_nb.set_width_request(260);
    
    toolbox.inspector.widget.set_visible(true);
    toolbox.inspector.widget.set_vexpand(true);
    right_nb.append_page(&toolbox.inspector.widget, Some(&Label::new(Some(" Properties "))));

    // ── AI Stack (Web AI, Claude, API Chat) ───────
    let ai_stack = gtk4::Stack::new();
    ai_stack.set_vexpand(true);
    ai_stack.set_hexpand(true);

    let ai_switcher = gtk4::StackSwitcher::new();
    ai_switcher.set_stack(Some(&ai_stack));
    ai_switcher.set_halign(gtk4::Align::Center);
    ai_switcher.set_margin_top(4);
    ai_switcher.set_margin_bottom(4);

    #[cfg(feature = "preview")]
    {
        ai_sidebar.widget.set_visible(true);
        ai_stack.add_titled(&ai_sidebar.widget, Some("web"), "Web AI");
    }
    claude_panel.widget.set_visible(true);
    ai_stack.add_titled(&claude_panel.widget, Some("claude"), "Claude");
    
    ai_chat_panel.widget.set_visible(true);
    ai_stack.add_titled(&ai_chat_panel.widget, Some("api"), "API Chat");

    let ai_box = GtkBox::new(Orientation::Vertical, 0);
    ai_box.append(&ai_switcher);
    ai_box.append(&ai_stack);

    right_nb.append_page(&ai_box, Some(&Label::new(Some(" AI Chat "))));

    // ── Session History tab ────────────────────────────────────────
    let chat_history = crate::chat_history::ChatHistoryPanel::new();
    right_nb.append_page(&chat_history.widget, Some(&Label::new(Some(" History "))));

    let right_paned = Paned::new(Orientation::Horizontal);
    right_paned.set_start_child(Some(&center_area));
    right_paned.set_end_child(Some(&right_nb));
    right_paned.set_resize_start_child(true);
    right_paned.set_resize_end_child(false);
    right_paned.set_shrink_start_child(false);
    right_paned.set_shrink_end_child(false);
    {
        let p = right_paned.clone();
        right_paned.connect_map(move |_| {
            let w = p.allocated_width();
            if w > 300 {
                p.set_position(w - 280);
            }
        });
    }

    let main_paned = Paned::new(Orientation::Horizontal);
    main_paned.set_start_child(Some(&left_nb));
    main_paned.set_end_child(Some(&right_paned));
    main_paned.set_resize_start_child(false);
    main_paned.set_resize_end_child(true);
    main_paned.set_shrink_start_child(false);
    main_paned.set_shrink_end_child(false);
    main_paned.set_position(230);

    // Lock left panel at fixed width — not user-resizable.
    main_paned.connect_notify_local(Some("position"), move |mp, _| {
        if mp.position() != 230 { mp.set_position(230); }
    });

    let vert_paned = Paned::new(Orientation::Vertical);
    vert_paned.set_start_child(Some(&main_paned));
    vert_paned.set_end_child(Some(&output.widget));
    vert_paned.set_resize_start_child(true);
    vert_paned.set_resize_end_child(false);
    vert_paned.set_position(580);
    if !cfg.show_output {
        output.widget.set_visible(false);
    }

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&vert_paned);
    root.append(&statusbar.widget);

    window.set_child(Some(&root));

    let state = Rc::new(RefCell::new(AppState {
        window:    window.clone(),
        notebook:  notebook.clone(),
        sidebar:   sidebar.clone(),
        output:    output.clone(),
        statusbar: statusbar.clone(),
        runner:    runner.clone(),
        find_bar:  find_bar.clone(),
        cfg:       cfg.clone(),
        canvas:    canvas.clone(),
        toolbox:   toolbox.clone(),
        outline:   outline.clone(),
        node_view: node_view.clone(),
        layout:    "code".into(),
        project_dir: None,
        history: Rc::new(RefCell::new(crate::history::DesignerHistory::new())),
        history_applying: Rc::new(Cell::new(false)),
        mru: crate::mru::Mru::load(),
        last_diagnostics: Rc::new(RefCell::new(Vec::new())),
        lsp: None,
        recent_files_menu: recent_files_menu.clone(),
        recent_projects_menu: recent_projects_menu.clone(),
        main_paned: main_paned.clone(),
        vert_paned: vert_paned.clone(),
        left_nb:   left_nb.clone(),
        center_nb: center_nb.clone(),
        right_nb:  right_nb.clone(),
        right_paned: right_paned.clone(),
        ai_stack:  ai_stack.clone(),
        minimap:   minimap.clone(),
        #[cfg(feature = "preview")]
        preview:   preview.clone(),
        #[cfg(feature = "preview")]
        ai_sidebar: ai_sidebar.clone(),
        claude_panel:   claude_panel.clone(),
        ai_chat_panel:  ai_chat_panel.clone(),
        chat_history:   chat_history.clone(),
    }));

    // Populate MRU menus from saved state.
    {
        let s = state.borrow();
        rebuild_mru_menus(&s.mru, &s.recent_files_menu, &s.recent_projects_menu);
    }

    // Wire Tree panel row-click → green canvas highlight.
    {
        let canvas_ref = canvas.clone();
        outline.on_row_select(move |byte_offset| {
            canvas_ref.select_from_tree(byte_offset);
        });
    }

    // Wire node-view widget click → jump to XML in the code editor.
    {
        let st = state.clone();
        node_view.on_jump_xml(move |char_offset| {
            let s = st.borrow();
            s.center_nb.set_current_page(Some(1));  // switch to Code tab
            if let Some(tab) = s.notebook.current_tab() {
                let buf = tab.buffer();
                let iter = buf.iter_at_offset(char_offset as i32);
                buf.place_cursor(&iter);
                tab.view.scroll_to_iter(&mut buf.iter_at_offset(char_offset as i32),
                    0.1, false, 0.0, 0.3);
            }
        });
    }

    // Wire node-view handler click → jump to line in companion .rs.
    {
        let st = state.clone();
        node_view.on_jump_rs(move |line_number| {
            let s = st.borrow();
            // Find an open tab whose path ends in .rs (prefer the companion)
            let rs_tab = s.notebook.all_tabs().into_iter()
                .enumerate()
                .find(|(_, t)| t.path().map(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("rs")
                }).unwrap_or(false));
            if let Some((idx, tab)) = rs_tab {
                s.notebook.widget.set_current_page(Some(idx as u32));
                s.center_nb.set_current_page(Some(1));
                let buf = tab.buffer();
                if line_number > 0 {
                    let ln = (line_number as i32 - 1).min(buf.line_count() - 1);
                    let iter = buf.iter_at_line(ln).unwrap_or_else(|| buf.start_iter());
                    buf.place_cursor(&iter);
                    tab.view.scroll_to_iter(
                        &mut buf.iter_at_line(ln).unwrap_or_else(|| buf.start_iter()),
                        0.1, false, 0.0, 0.3);
                }
            }
        });
    }

    // Wire outline rebuild → inspector selector combo.
    {
        let inspector_ref = toolbox.inspector.clone();
        outline.on_rebuild(move |items| {
            inspector_ref.populate_selector(items);
        });
    }

    // Wire inspector selector combo selection → canvas highlight + inspector.
    {
        let canvas_ref    = canvas.clone();
        let inspector_ref = toolbox.inspector.clone();
        toolbox.inspector.on_select_widget(move |byte_offset| {
            canvas_ref.select_from_tree(byte_offset);
            inspector_ref.update_from_offset(byte_offset);
        });
    }

    // Wire inspector selector trash button → delete widget from buffer.
    {
        let outline_ref = outline.clone();
        toolbox.inspector.on_delete_widget(move |start, end| {
            outline_ref.delete_child_bytes(start, end);
        });
    }

    // Wire canvas widget selection → palette insert target.
    // When a canvas widget gets the blue highlight, palette inserts land there.
    {
        let palette_ref = toolbox.palette.clone();
        canvas.on_widget_select(move |byte_offset, grid_cell| {
            palette_ref.set_insert_target(byte_offset, grid_cell);
        });
    }

    // ── Connect notebook tab-switch → statusbar + minimap + canvas ──
    // Wire clickable error locations in the output panel
    {
        let st = state.clone();
        output.set_on_error_click(move |file, line, _col| {
            let s = st.borrow();
            // Resolve file path relative to project_dir if set
            let abs_path = if let Some(ref pd) = s.project_dir {
                pd.join(&file)
            } else {
                // Try relative to current tab's directory
                s.notebook.current_tab()
                    .and_then(|t| t.path())
                    .and_then(|p| p.parent().map(|d| d.join(&file)))
                    .unwrap_or(file)
            };
            if abs_path.exists() {
                s.notebook.open_file(&abs_path, &s.cfg);
                // Jump to the error line
                if let Some(tab) = s.notebook.current_tab() {
                    let target_line = (line - 1).max(0);
                    let buf = tab.buffer();
                    if let Some(iter) = buf.iter_at_line(target_line) {
                        buf.place_cursor(&iter);
                        // Scroll to cursor
                        tab.view.scroll_to_iter(&mut buf.iter_at_mark(&buf.get_insert()), 0.1, false, 0.0, 0.5);
                    }
                }
            }
        });
    }

    // Wire robot button in output panel → active AI panel input
    {
        let st = state.clone();
        output.set_on_ask_ai(move |errors| {
            let s = st.borrow();
            let prompt = format!("I got these compile errors, can you help fix them?\nPlease respond with a unified ```diff block so I can review and apply it.\n\n```\n{}\n```", errors);
            match s.ai_stack.visible_child_name().as_deref() {
                Some("claude") => s.claude_panel.set_input(&prompt),
                Some("api")    => s.ai_chat_panel.set_input(&prompt),
                _              => s.claude_panel.set_input(&prompt),
            }
        });
    }

    {
        let st = state.clone();
        notebook.on_switch(move |_, tab| {
            let s = st.borrow();
            s.statusbar.connect_tab(tab);
            s.find_bar.set_view(&tab.view);
            s.minimap.set_view(&tab.view);
            s.toolbox.set_buffer(&tab.buffer());
            // Notify LSP of the newly-focused document; wire debounced change notifications
            if let Some(ref lsp) = s.lsp {
                if let Some(path) = tab.path() {
                    if path.extension().map(|e| e == "rs").unwrap_or(false) {
                        let buf = tab.buffer();
                        let (start, end) = buf.bounds();
                        let text = buf.text(&start, &end, false).to_string();
                        lsp.notify_open(&path, &text);

                        // Debounced notify_change: 400 ms after last keystroke
                        let lsp2 = lsp.clone();
                        let path2 = path.clone();
                        let gen_rc = Rc::new(Cell::new(0u64));
                        let gen2 = gen_rc.clone();
                        buf.connect_changed(move |buf| {
                            if !lsp2.initialized.get() { return; }
                            let gen = gen2.get().wrapping_add(1);
                            gen2.set(gen);
                            let lsp3 = lsp2.clone();
                            let path3 = path2.clone();
                            let gen3 = gen2.clone();
                            let (s2, e2) = buf.bounds();
                            let text = buf.text(&s2, &e2, false).to_string();
                            gtk4::glib::timeout_add_local_once(
                                std::time::Duration::from_millis(400),
                                move || {
                                    if gen3.get() == gen {
                                        lsp3.notify_change(&path3, &text);
                                    }
                                },
                            );
                        });
                    }
                }
            }

            // Connect canvas, toolbox, and outline if this is a .ui file
            if let Some(path) = tab.path() {
                if Canvas::is_ui_file(&path) {
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
                    s.node_view.connect_buffer(&tab.buffer());
                    // Feed companion .rs content for signal-edge parsing
                    let rs_content = crate::codegen::companion_path(&path);
                    let rs_text = std::fs::read_to_string(&rs_content).unwrap_or_default();
                    s.node_view.set_rs_content(&rs_text);
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
                    // Reset history, snapshot the initial state, then attach
                    // the debounced hook.  The initial push means there is
                    // always a "pristine" baseline to undo all the way back to.
                    s.history.borrow_mut().reset();
                    s.history.borrow_mut().push(&text);
                    {
                        let hist = s.history.clone();
                        let applying = s.history_applying.clone();
                        let cnb = s.center_nb.clone();
                        let gen_rc = Rc::new(Cell::new(0u64));
                        let gen2 = gen_rc.clone();
                        tab.buffer().connect_changed(move |buf| {
                            if applying.get() { return; }
                            if cnb.current_page() != Some(0) { return; }
                            let gen = gen2.get().wrapping_add(1);
                            gen2.set(gen);
                            let hist2 = hist.clone();
                            let gen3 = gen2.clone();
                            let buf2 = buf.clone();
                            gtk4::glib::timeout_add_local_once(
                                std::time::Duration::from_millis(800),
                                move || {
                                    if gen3.get() == gen {
                                        let (s, e) = buf2.bounds();
                                        let text = buf2.text(&s, &e, false).to_string();
                                        hist2.borrow_mut().push(&text);
                                    }
                                },
                            );
                        });
                    }
                } else {
                    s.canvas.clear();
                    s.outline.clear_buffer();
                    s.node_view.clear();
                }
            } else {
                s.canvas.clear();
                s.outline.clear_buffer();
                s.node_view.clear();
            }
        });
    }

    // ── Connect sidebar file-open → notebook ──────────────────────
    {
        let st = state.clone();
        sidebar.on_open(move |path| {
            let s = st.borrow();
            s.notebook.open_file(&path, &s.cfg);
            // Always update toolbox buffer
            if let Some(tab) = s.notebook.current_tab() {
                s.toolbox.set_buffer(&tab.buffer());
            }
            // Activate canvas + toolbox + outline + node_view if this is a .ui file
            if Canvas::is_ui_file(&path) {
                if let Some(tab) = s.notebook.current_tab() {
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
                    s.node_view.connect_buffer(&tab.buffer());
                    let rs_content = crate::codegen::companion_path(&path);
                    let rs_text = std::fs::read_to_string(&rs_content).unwrap_or_default();
                    s.node_view.set_rs_content(&rs_text);
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
                    // Switch to Design tab and Widgets panel
                    s.center_nb.set_current_page(Some(0));
                    s.left_nb.set_current_page(Some(1));
                }
            }
            #[cfg(feature = "preview")]
            {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "html" | "htm" | "css") && s.preview.is_visible_pref() {
                    s.preview.reload_from_file(&path);
                }
            }
        });
    }

    // ── Open files (CLI args or session restore) ──────────────────
    {
        let (files_to_open, saved_layout) = if open_paths.is_empty() {
            let (files, layout) = crate::session::load();
            (files, layout)
        } else {
            (open_paths.clone(), "code".into())
        };

        // Apply saved layout
        {
            let mut s = state.borrow_mut();
            s.layout = saved_layout.clone();
        }

        let s = state.borrow();
        if saved_layout == "designer" {
            s.left_nb.set_current_page(Some(1));
            s.center_nb.set_current_page(Some(0));
            s.minimap.set_visible(false);
        }

        let startup_dir = files_to_open
            .first()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| crate::config::startup_dir(&cfg));
        s.sidebar.set_root(&startup_dir);

        for path in &files_to_open {
            s.notebook.open_file(path, &s.cfg);
        }

        if let Some(tab) = s.notebook.current_tab() {
            s.minimap.set_view(&tab.view);
            s.toolbox.set_buffer(&tab.buffer());
            // If the last opened file is a .ui, activate canvas + toolbox + outline
            if let Some(path) = tab.path() {
                if Canvas::is_ui_file(&path) {
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
                    s.node_view.connect_buffer(&tab.buffer());
                    let rs_text = std::fs::read_to_string(
                        &crate::codegen::companion_path(&path)
                    ).unwrap_or_default();
                    s.node_view.set_rs_content(&rs_text);
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
                    s.center_nb.set_current_page(Some(0));
                    s.left_nb.set_current_page(Some(1));
                }
            }
        }

        if s.notebook.tab_count() == 0 {
            s.notebook.new_tab(&s.cfg);
        }
    }

    // ── Wire menu actions ─────────────────────────────────────────

    // File → New Tab
    {
        let st = state.clone();
        menubar::connect_action(app, "new-tab", move || {
            let s = st.borrow();
            s.notebook.new_tab(&s.cfg);
        });
    }

    // File → New .ui File
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "new-ui", move || {
            let st2 = st.clone();
            show_layout_dialog(&win, move |template| {
                let s = st2.borrow();
                s.notebook.close_all();
                s.canvas.clear();
                s.notebook.new_tab(&s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(&template);
                    if let Some(lang) = sourceview5::LanguageManager::default().language("xml") {
                        tab.buffer().set_language(Some(&lang));
                    }
                    tab.buffer().set_modified(false);
                    s.toolbox.set_buffer(&tab.buffer());
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
                    s.node_view.connect_buffer(&tab.buffer());
                    s.node_view.set_rs_content("");
                    s.canvas.render(&template);
                    s.center_nb.set_current_page(Some(0));
                    s.left_nb.set_current_page(Some(1));
                }
            });
        });
    }

    // File → New Project
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "new-project", move || {
            let st2 = st.clone();
            let win2 = win.clone();
            let file_dialog = FileDialog::builder()
                .title("New Project — Select or Create a Folder")
                .modal(true)
                .build();
            file_dialog.select_folder(Some(&win), gtk4::gio::Cancellable::NONE, move |result| {
                if let Ok(folder) = result {
                    if let Some(dir) = folder.path() {
                        let project_name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "my_app".into())
                            .replace(' ', "_")
                            .to_lowercase();

                        // Show layout dialog before scaffolding
                        let st3 = st2.clone();
                        show_layout_dialog(&win2, move |ui| {
                            if let Err(e) = crate::codegen::scaffold_project(&dir, &project_name, &ui) {
                                log::error!("Failed to scaffold project: {}", e);
                                return;
                            }

                            st3.borrow_mut().project_dir = Some(dir.clone());
                            start_lsp(&st3, &dir);

                            let s = st3.borrow();
                            s.notebook.close_all();
                            s.canvas.clear();
                            s.sidebar.set_root(&dir);

                            let ui_path = dir.join("layout.ui");
                            s.notebook.open_file(&ui_path, &s.cfg);

                            if let Some(tab) = s.notebook.current_tab() {
                                s.toolbox.set_buffer(&tab.buffer());
                                s.canvas.connect_buffer(&tab.buffer());
                                s.toolbox.connect_buffer(&tab.buffer());
                                s.outline.connect_buffer(&tab.buffer());
                                s.node_view.connect_buffer(&tab.buffer());
                                let rs_text = std::fs::read_to_string(
                                    &crate::codegen::companion_path(&ui_path)
                                ).unwrap_or_default();
                                s.node_view.set_rs_content(&rs_text);
                                let (start, end) = tab.buffer().bounds();
                                let text = tab.buffer().text(&start, &end, false).to_string();
                                s.canvas.render(&text);
                                s.center_nb.set_current_page(Some(0));
                                s.left_nb.set_current_page(Some(1));
                            }

                            let main_path = dir.join("src").join("main.rs");
                            s.notebook.open_file(&main_path, &s.cfg);
                            s.notebook.open_file(&ui_path, &s.cfg);

                            s.output.append_run_line(&format!(
                                "✓ Created project \"{}\" → {}",
                                project_name,
                                dir.display()
                            ));
                            s.output.show_panel();
                        });
                    }
                }
            });
        });
    }

    // File → Open
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "open", move || {
            let st2 = st.clone();
            let dialog = FileDialog::builder()
                .title("Open File")
                .modal(true)
                .build();
            dialog.open(Some(&win), gtk4::gio::Cancellable::NONE, move |result: Result<gtk4::gio::File, gtk4::glib::Error>| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        {
                            let s = st2.borrow();
                            s.notebook.open_file(&path, &s.cfg);
                            // Always update toolbox buffer for the new tab
                            if let Some(tab) = s.notebook.current_tab() {
                                s.toolbox.set_buffer(&tab.buffer());
                            }
                            // Activate canvas + toolbox + outline if this is a .ui file
                            if Canvas::is_ui_file(&path) {
                                if let Some(tab) = s.notebook.current_tab() {
                                    s.canvas.connect_buffer(&tab.buffer());
                                    s.toolbox.connect_buffer(&tab.buffer());
                                    s.outline.connect_buffer(&tab.buffer());
                                    s.node_view.connect_buffer(&tab.buffer());
                                    let rs_text = std::fs::read_to_string(
                                        &crate::codegen::companion_path(&path)
                                    ).unwrap_or_default();
                                    s.node_view.set_rs_content(&rs_text);
                                    let (start, end) = tab.buffer().bounds();
                                    let text = tab.buffer().text(&start, &end, false).to_string();
                                    s.canvas.render(&text);
                                    s.center_nb.set_current_page(Some(0));
                                    s.left_nb.set_current_page(Some(1));
                                }
                            }
                        }
                        // Track in MRU
                        { st2.borrow_mut().mru.add_file(path.clone()); }
                        {
                            let s = st2.borrow();
                            rebuild_mru_menus(&s.mru, &s.recent_files_menu, &s.recent_projects_menu);
                        }
                    }
                }
            });
        });
    }

    // File → Open Project
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "open-project", move || {
            let st2 = st.clone();
            let dialog = FileDialog::builder()
                .title("Open Project Folder")
                .modal(true)
                .build();
            dialog.select_folder(Some(&win), gtk4::gio::Cancellable::NONE, move |result| {
                if let Ok(folder) = result {
                    if let Some(dir) = folder.path() {
                        // Remember project directory for Run/Build
                        st2.borrow_mut().project_dir = Some(dir.clone());
                        start_lsp(&st2, &dir);

                        {
                            let s = st2.borrow();
                            s.notebook.close_all();
                            s.canvas.clear();
                            s.sidebar.set_root(&dir);

                            // Auto-open layout.ui if it exists
                            let ui_path = dir.join("layout.ui");
                            if ui_path.exists() {
                                s.notebook.open_file(&ui_path, &s.cfg);
                                if let Some(tab) = s.notebook.current_tab() {
                                    s.toolbox.set_buffer(&tab.buffer());
                                    s.canvas.connect_buffer(&tab.buffer());
                                    s.toolbox.connect_buffer(&tab.buffer());
                                    s.outline.connect_buffer(&tab.buffer());
                                    s.node_view.connect_buffer(&tab.buffer());
                                    let rs_text = std::fs::read_to_string(
                                        &crate::codegen::companion_path(&ui_path)
                                    ).unwrap_or_default();
                                    s.node_view.set_rs_content(&rs_text);
                                    let (start, end) = tab.buffer().bounds();
                                    let text = tab.buffer().text(&start, &end, false).to_string();
                                    s.canvas.render(&text);
                                    s.center_nb.set_current_page(Some(0));
                                    s.left_nb.set_current_page(Some(1));
                                }
                            }

                            // Auto-open all .rs files in src/
                            let src_dir = dir.join("src");
                            if src_dir.is_dir() {
                                // Collect and sort so main.rs, layout_app.rs come in predictable order
                                let mut rs_files: Vec<std::path::PathBuf> = Vec::new();
                                if let Ok(entries) = std::fs::read_dir(&src_dir) {
                                    for entry in entries.flatten() {
                                        let p = entry.path();
                                        if p.is_file() {
                                            if let Some(ext) = p.extension() {
                                                if ext == "rs" {
                                                    rs_files.push(p);
                                                }
                                            }
                                        }
                                    }
                                }
                                // Sort: main.rs first, then layout_app.rs, then rest alphabetically
                                rs_files.sort_by(|a, b| {
                                    let name_a = a.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    let name_b = b.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    let rank = |n: &str| -> u8 {
                                        if n == "main.rs" { 0 }
                                        else if n == "layout_app.rs" { 1 }
                                        else { 2 }
                                    };
                                    rank(&name_a).cmp(&rank(&name_b))
                                        .then_with(|| name_a.cmp(&name_b))
                                });
                                for rs_path in &rs_files {
                                    s.notebook.open_file(rs_path, &s.cfg);
                                }
                            }

                            // Switch back to layout.ui tab if it was opened
                            if ui_path.exists() {
                                s.notebook.open_file(&ui_path, &s.cfg);
                            }

                            s.output.append_run_line(&format!(
                                "✓ Opened project → {}",
                                dir.display()
                            ));
                            s.output.show_panel();
                        }
                        // Track in MRU
                        { st2.borrow_mut().mru.add_project(dir.clone()); }
                        {
                            let s = st2.borrow();
                            rebuild_mru_menus(&s.mru, &s.recent_files_menu, &s.recent_projects_menu);
                        }
                    }
                }
            });
        });
    }

    // File → Open Recent File (parameterized action with path as string variant)
    {
        let st = state.clone();
        let action = gtk4::gio::SimpleAction::new(
            "open-recent-file",
            Some(gtk4::glib::VariantTy::STRING),
        );
        action.connect_activate(move |_, param| {
            if let Some(path_str) = param.and_then(|v| v.get::<String>()) {
                let path = PathBuf::from(&path_str);
                {
                    let s = st.borrow();
                    s.notebook.open_file(&path, &s.cfg);
                    if let Some(tab) = s.notebook.current_tab() {
                        s.toolbox.set_buffer(&tab.buffer());
                    }
                    if Canvas::is_ui_file(&path) {
                        if let Some(tab) = s.notebook.current_tab() {
                            s.canvas.connect_buffer(&tab.buffer());
                            s.toolbox.connect_buffer(&tab.buffer());
                            s.outline.connect_buffer(&tab.buffer());
                            s.node_view.connect_buffer(&tab.buffer());
                            let rs_text = std::fs::read_to_string(
                                &crate::codegen::companion_path(&path)
                            ).unwrap_or_default();
                            s.node_view.set_rs_content(&rs_text);
                            let (start, end) = tab.buffer().bounds();
                            let text = tab.buffer().text(&start, &end, false).to_string();
                            s.canvas.render(&text);
                            s.center_nb.set_current_page(Some(0));
                            s.left_nb.set_current_page(Some(1));
                        }
                    }
                }
                // Promote to front of MRU
                { st.borrow_mut().mru.add_file(path); }
                {
                    let s = st.borrow();
                    rebuild_mru_menus(&s.mru, &s.recent_files_menu, &s.recent_projects_menu);
                }
            }
        });
        app.add_action(&action);
    }

    // File → Open Recent Project (parameterized action with path as string variant)
    {
        let st = state.clone();
        let action = gtk4::gio::SimpleAction::new(
            "open-recent-project",
            Some(gtk4::glib::VariantTy::STRING),
        );
        action.connect_activate(move |_, param| {
            if let Some(path_str) = param.and_then(|v| v.get::<String>()) {
                let dir = PathBuf::from(&path_str);
                if !dir.is_dir() { return; }

                st.borrow_mut().project_dir = Some(dir.clone());
                start_lsp(&st, &dir);
                {
                    let s = st.borrow();
                    s.sidebar.set_root(&dir);

                    let ui_path = dir.join("layout.ui");
                    if ui_path.exists() {
                        s.notebook.open_file(&ui_path, &s.cfg);
                        if let Some(tab) = s.notebook.current_tab() {
                            s.toolbox.set_buffer(&tab.buffer());
                            s.canvas.connect_buffer(&tab.buffer());
                            s.toolbox.connect_buffer(&tab.buffer());
                            s.outline.connect_buffer(&tab.buffer());
                            s.node_view.connect_buffer(&tab.buffer());
                            let rs_text = std::fs::read_to_string(
                                &crate::codegen::companion_path(&ui_path)
                            ).unwrap_or_default();
                            s.node_view.set_rs_content(&rs_text);
                            let (start, end) = tab.buffer().bounds();
                            let text = tab.buffer().text(&start, &end, false).to_string();
                            s.canvas.render(&text);
                            s.center_nb.set_current_page(Some(0));
                            s.left_nb.set_current_page(Some(1));
                        }
                    }
                    s.output.append_run_line(&format!("✓ Opened project → {}", dir.display()));
                    s.output.show_panel();
                }
                // Promote to front of MRU
                { st.borrow_mut().mru.add_project(dir); }
                {
                    let s = st.borrow();
                    rebuild_mru_menus(&s.mru, &s.recent_files_menu, &s.recent_projects_menu);
                }
            }
        });
        app.add_action(&action);
    }

    // File → Save
    {
        let st = state.clone();
        menubar::connect_action(app, "save", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                if let Some(path) = tab.path() {
                    let _ = s.notebook.save_current();
                    validate_ui_tab(&tab, &s.output);
                    // rustfmt on save for .rs files
                    if path.extension().map(|e| e == "rs").unwrap_or(false) {
                        let tab2   = tab.clone();
                        let diags  = s.last_diagnostics.clone();
                        crate::runner::RunManager::rustfmt_file(path, move |result| {
                            if let Ok(()) = result {
                                if let Ok(content) = std::fs::read_to_string(tab2.path().unwrap()) {
                                    let buf = tab2.buffer();
                                    let cursor_line = buf.iter_at_mark(&buf.get_insert()).line();
                                    buf.set_text(&content);
                                    buf.set_modified(false);
                                    *tab2.modified.borrow_mut() = false;
                                    if let Some(mut iter) = buf.iter_at_line(cursor_line) {
                                        buf.place_cursor(&iter);
                                    }
                                    // Re-apply inline marks after format
                                    let d = diags.borrow();
                                    diagnostics::apply_to_tab(&tab2, &d);
                                }
                            }
                        });
                    }
                } else {
                    drop(s);
                    let _ = gtk4::prelude::WidgetExt::activate_action(
                        &st.borrow().window, "app.save-as", None
                    );
                }
            }
        });
    }

    // File → Save All
    {
        let st = state.clone();
        menubar::connect_action(app, "save-all", move || {
            let s = st.borrow();
            let tabs = s.notebook.all_tabs();
            let saveable: Vec<_> = tabs.iter()
                .filter(|t| t.path().is_some())
                .collect();

            if saveable.is_empty() {
                s.output.append_run_line("Save All: no open files with a path.");
                return;
            }

            let mut saved = 0usize;
            let mut failed = 0usize;
            for tab in &saveable {
                let name = tab.path()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "?".to_string());
                match tab.save() {
                    Ok(()) => {
                        saved += 1;
                        s.output.append_run_line(&format!("  ✓ {}", name));
                        validate_ui_tab(tab, &s.output);
                    }
                    Err(e) => {
                        log::error!("Save All: {}", e);
                        s.output.append_run_error(&format!("  ✗ {} — {}", name, e));
                        failed += 1;
                    }
                }
            }

            if failed > 0 {
                s.output.append_run_error(&format!(
                    "Save All: {} saved, {} failed.", saved, failed
                ));
            } else {
                s.output.append_run_line(&format!(
                    "✓ Save All: {} file{} saved.", saved, if saved == 1 { "" } else { "s" }
                ));
            }
        });
    }

    // File → Save As
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "save-as", move || {
            let st2 = st.clone();
            let dialog = FileDialog::builder()
                .title("Save As")
                .modal(true)
                .build();
            dialog.save(Some(&win), gtk4::gio::Cancellable::NONE, move |result: Result<gtk4::gio::File, gtk4::glib::Error>| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let s = st2.borrow();
                        let _ = s.notebook.save_current_to(&path);
                    }
                }
            });
        });
    }

    // File → Close Tab
    {
        let st = state.clone();
        menubar::connect_action(app, "close-tab", move || {
            let s = st.borrow();
            // Notify LSP that the document is being closed
            if let Some(ref lsp) = s.lsp {
                if let Some(tab) = s.notebook.current_tab() {
                    if let Some(path) = tab.path() {
                        if path.extension().map(|e| e == "rs").unwrap_or(false) {
                            lsp.notify_close(&path);
                        }
                    }
                }
            }
            s.notebook.close_current();
        });
    }

    // Edit → Cut / Copy / Paste / Select All
    {
        let st = state.clone();
        menubar::connect_action(app, "cut", move || {
            if let Some(tab) = st.borrow().notebook.current_tab() {
                let _ = tab.view.activate_action("text.cut-clipboard", None);
            }
        });
    }
    {
        let st = state.clone();
        menubar::connect_action(app, "copy", move || {
            if let Some(tab) = st.borrow().notebook.current_tab() {
                let _ = tab.view.activate_action("text.copy-clipboard", None);
            }
        });
    }
    {
        let st = state.clone();
        menubar::connect_action(app, "paste", move || {
            if let Some(tab) = st.borrow().notebook.current_tab() {
                let _ = tab.view.activate_action("text.paste-clipboard", None);
            }
        });
    }
    {
        let st = state.clone();
        menubar::connect_action(app, "select-all", move || {
            if let Some(tab) = st.borrow().notebook.current_tab() {
                let _ = tab.view.activate_action("text.select-all", None);
            }
        });
    }

    // Edit → Undo (Ctrl+Z): designer history in Design mode, SourceView undo in Code mode
    {
        let st = state.clone();
        menubar::connect_action(app, "designer-undo", move || {
            let s = st.borrow();
            if s.center_nb.current_page() == Some(0) {
                // Design mode: restore previous XML snapshot
                let xml = s.history.borrow_mut().undo();
                if let Some(xml) = xml {
                    s.history_applying.set(true);
                    if let Some(tab) = s.notebook.current_tab() {
                        let buf = tab.buffer();
                        buf.begin_user_action();
                        let (mut start, mut end) = (buf.start_iter(), buf.end_iter());
                        buf.delete(&mut start, &mut end);
                        buf.insert(&mut buf.end_iter(), &xml);
                        buf.end_user_action();
                    }
                    s.history_applying.set(false);
                }
            } else {
                // Code mode: delegate to SourceView's built-in undo
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().undo();
                }
            }
        });
    }

    // Edit → Redo (Ctrl+Y / Ctrl+Shift+Z): designer history in Design mode, SourceView redo in Code mode
    {
        let st = state.clone();
        menubar::connect_action(app, "designer-redo", move || {
            let s = st.borrow();
            if s.center_nb.current_page() == Some(0) {
                // Design mode: restore next XML snapshot
                let xml = s.history.borrow_mut().redo();
                if let Some(xml) = xml {
                    s.history_applying.set(true);
                    if let Some(tab) = s.notebook.current_tab() {
                        let buf = tab.buffer();
                        buf.begin_user_action();
                        let (mut start, mut end) = (buf.start_iter(), buf.end_iter());
                        buf.delete(&mut start, &mut end);
                        buf.insert(&mut buf.end_iter(), &xml);
                        buf.end_user_action();
                    }
                    s.history_applying.set(false);
                }
            } else {
                // Code mode: delegate to SourceView's built-in redo
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().redo();
                }
            }
        });
    }

    // Edit → Find
    {
        let st = state.clone();
        menubar::connect_action(app, "find", move || {
            st.borrow().find_bar.reveal();
        });
    }

    // Edit → Find & Replace
    {
        let st = state.clone();
        menubar::connect_action(app, "find-replace", move || {
            st.borrow().find_bar.reveal_replace();
        });
    }

    // F12 → Go to Definition
    {
        let st = state.clone();
        menubar::connect_action(app, "goto-definition", move || {
            let s = st.borrow();
            let Some(ref lsp) = s.lsp else { return };
            let Some(tab) = s.notebook.current_tab() else { return };
            let Some(path) = tab.path() else { return };
            let buf = tab.buffer();
            let cursor = buf.iter_at_mark(&buf.get_insert());
            let line = cursor.line() as u32;
            let col  = cursor.line_offset() as u32;
            let notebook = s.notebook.clone();
            let cfg      = s.cfg.clone();
            lsp.request_definition(&path, line, col, move |result| {
                let Some((def_path, def_line, def_col)) = result else { return };
                notebook.open_file(&def_path, &cfg);
                // Jump to the definition line
                if let Some(tab) = notebook.current_tab() {
                    let buf = tab.buffer();
                    if let Some(mut iter) = buf.iter_at_line(def_line as i32) {
                        iter.set_line_offset(def_col as i32);
                        buf.place_cursor(&iter);
                        tab.view.scroll_to_iter(&mut iter.clone(), 0.1, true, 0.5, 0.3);
                    }
                }
            });
        });
    }

    // Edit → Search in Files
    {
        let st = state.clone();
        menubar::connect_action(app, "search-in-files", move || {
            st.borrow().output.focus_search();
        });
    }

    // Wire the Search tab's submit button → run ripgrep
    {
        let st = state.clone();
        let output = output.clone();
        output.set_on_search_request(move |query| {
            let s = st.borrow();
            let dir = s.project_dir.clone()
                .filter(|pd| pd.exists())
                .or_else(|| s.notebook.current_tab().and_then(|t| t.path())
                    .and_then(|p| find_cargo_toml_dir(&p)))
                .or_else(|| s.notebook.current_tab().and_then(|t| t.path())
                    .and_then(|p| p.parent().map(|d| d.to_path_buf())));
            let Some(dir) = dir else {
                s.output.set_search_results(query, &[]);
                return;
            };
            let output2 = s.output.clone();
            let q = query.to_string();
            crate::runner::RunManager::run_ripgrep(q.clone(), dir, move |matches| {
                output2.set_search_results(&q, &matches);
            });
        });
    }

    // Edit → Go to Line
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "goto-line", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                goto::show_goto_dialog(&win, &tab.buffer());
            }
        });
    }

    // View → Toggle Sidebar
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-sidebar", move || {
            let s = st.borrow();
            let vis = s.left_nb.is_visible();
            s.left_nb.set_visible(!vis);
        });
    }

    // View → Toggle Output
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-output", move || {
            st.borrow().output.toggle();
        });
    }

    // View → Toggle Canvas (live .ui preview)
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-canvas", move || {
            let s = st.borrow();
            s.center_nb.set_current_page(Some(0));
            s.left_nb.set_current_page(Some(1));
        });
    }

    // View → Toggle Toolbox (palette + inspector)
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-toolbox", move || {
            let s = st.borrow();
            s.left_nb.set_current_page(Some(1));
        });
    }

    // View → Toggle Preview
    #[cfg(feature = "preview")]
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-preview", move || {
            st.borrow().preview.toggle();
        });
    }
    #[cfg(not(feature = "preview"))]
    {
        menubar::connect_action(app, "toggle-preview", move || {});
    }

    // View → Toggle Minimap
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-minimap", move || {
            let s = st.borrow();
            s.minimap.set_visible(!s.minimap.is_visible());
        });
    }

    // View → Toggle Dark Mode
    {
        use gtk4::glib::variant::ToVariant;
        let app_weak = gtk4::glib::object::ObjectExt::downgrade(app);
        let dark_btn_ref = dark_btn.clone();
        let dark_state_ref = dark_state.clone();
        let st = state.clone();
        menubar::connect_action(app, "toggle-dark", move || {
            let now_dark = !dark_state_ref.get();
            dark_state_ref.set(now_dark);
            if let Some(settings) = gtk4::Settings::default() {
                settings.set_gtk_application_prefer_dark_theme(now_dark);
            }
            // Update toolbar button icon: moon = enable dark, sun = enable light
            dark_btn_ref.set_label(if now_dark { "\u{f185}" } else { "\u{f186}" });
            dark_btn_ref.set_tooltip_text(Some(if now_dark { "Switch to Light Mode" } else { "Switch to Dark Mode" }));
            // Update action state so the menu shows a checkmark
            if let Some(app) = app_weak.upgrade() {
                if let Some(action) = app.lookup_action("toggle-dark") {
                    if let Some(sa) = action.downcast_ref::<gtk4::gio::SimpleAction>() {
                        sa.set_state(&now_dark.to_variant());
                    }
                }
            }
            // Persist to rui.toml
            let mut editor_cfg = crate::config::load();
            editor_cfg.dark_mode = now_dark;
            crate::config::save_editor_config(&editor_cfg);
            // Re-apply colour scheme to all open tabs so SourceView matches the GTK theme.
            // Dark mode → use the configured scheme; light mode → clear to GtkSourceView auto.
            let scheme_id = if now_dark { editor_cfg.color_scheme.as_str() } else { "" };
            let s = st.borrow();
            for tab in s.notebook.all_tabs() {
                tab.apply_scheme(scheme_id);
            }
        });
    }

    // View → Layouts → Code View
    // Code-focused: full sidebar, no canvas, no palette, no preview, minimap on
    {
        let st = state.clone();
        menubar::connect_action(app, "layout-code", move || {
            let mut s = st.borrow_mut();
            s.layout = "code".into();
            s.left_nb.set_visible(true);
            s.left_nb.set_current_page(Some(0)); // Files
            s.center_nb.set_current_page(Some(1)); // Code
            s.minimap.set_visible(true);
            s.output.widget.set_visible(true);
        });
    }

    // View → Layouts → Designer View
    // Design-focused: narrow sidebar, canvas + palette visible, no preview
    {
        let st = state.clone();
        menubar::connect_action(app, "layout-designer", move || {
            let mut s = st.borrow_mut();
            s.layout = "designer".into();
            s.left_nb.set_visible(true);
            s.left_nb.set_current_page(Some(1)); // Widgets
            s.center_nb.set_current_page(Some(0)); // Design
            s.minimap.set_visible(false);
            s.output.widget.set_visible(false);
        });
    }

    // Run → Run
    {
        let st = state.clone();
        menubar::connect_action(app, "run", move || {
            let s = st.borrow();
            // Save all modified files before running
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() {
                    let _ = tab.save();
                }
            }
            // If a project dir is set with a Cargo.toml, use that
            if let Some(ref pd) = s.project_dir {
                if pd.join("Cargo.toml").exists() {
                    s.runner.run_in_dir(pd, &["run"], &s.output);
                    return;
                }
            }
            // Fallback: run the current file
            if let Some(tab) = s.notebook.current_tab() {
                if let Some(path) = tab.path() {
                    let output = s.output.clone();
                    let runner = s.runner.clone();
                    runner.run_file(
                        &path,
                        &output,
                        || {},
                        |_success| {},
                    );
                } else {
                    s.output.append_run_error("Save the file before running.");
                    s.output.show_panel();
                }
            }
        });
    }

    // Run → Build
    {
        let st = state.clone();
        menubar::connect_action(app, "build", move || {
            let s = st.borrow();
            // Save all modified files before building
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() {
                    let _ = tab.save();
                }
            }
            // Resolve cargo directory: prefer project_dir, then walk up from current file
            let cargo_dir = s.project_dir.clone()
                .filter(|pd| pd.join("Cargo.toml").exists())
                .or_else(|| {
                    s.notebook.current_tab()
                        .and_then(|t| t.path())
                        .and_then(|p| find_cargo_toml_dir(&p))
                });

            if let Some(cd) = cargo_dir {
                s.runner.check_then_build_in_dir(&cd, &["build", "--release"], &s.output);
            } else if let Some(tab) = s.notebook.current_tab() {
                if let Some(path) = tab.path() {
                    let output = s.output.clone();
                    let runner = s.runner.clone();
                    runner.run_file(&path, &output, || {}, |_| {});
                }
            }
        });
    }

    // Run → Build Install
    {
        let st = state.clone();
        menubar::connect_action(app, "build-install", move || {
            let s = st.borrow();
            // Save all modified files
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() {
                    let _ = tab.save();
                }
            }
            let cargo_dir = s.project_dir.clone()
                .filter(|pd| pd.join("Cargo.toml").exists())
                .or_else(|| {
                    s.notebook.current_tab()
                        .and_then(|t| t.path())
                        .and_then(|p| find_cargo_toml_dir(&p))
                });

            if let Some(cd) = cargo_dir {
                // Generate .desktop and install.sh before building
                if let Err(e) = generate_install_files(&cd, &s.output) {
                    s.output.append_run_error(&format!("Failed to generate install files: {}", e));
                    return;
                }
                s.runner.check_then_build_in_dir(&cd, &["build", "--release"], &s.output);
            } else {
                s.output.append_run_error("No Cargo.toml found — open a project first.");
                s.output.show_panel();
            }
        });
    }

    // Run → Stop
    {
        let st = state.clone();
        menubar::connect_action(app, "stop", move || {
            st.borrow().runner.stop();
        });
    }

    // Run → Check
    {
        let st = state.clone();
        menubar::connect_action(app, "check", move || {
            let s = st.borrow();
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() { let _ = tab.save(); }
            }
            let cargo_dir = s.project_dir.clone()
                .filter(|pd| pd.join("Cargo.toml").exists())
                .or_else(|| s.notebook.current_tab().and_then(|t| t.path()).and_then(|p| find_cargo_toml_dir(&p)));
            let Some(cd) = cargo_dir else {
                s.output.append_run_error("No Cargo.toml found — open a project first.");
                s.output.show_panel();
                return;
            };
            let notebook  = s.notebook.clone();
            let output    = s.output.clone();
            let last_diag = s.last_diagnostics.clone();
            let cd2 = cd.clone();
            s.runner.run_in_dir_json(&cd, &["check"], &s.output, move |json_lines| {
                let diags = diagnostics::parse_cargo_json(&json_lines, &cd2);
                *last_diag.borrow_mut() = diags.clone();
                output.set_diagnostics(&diags);
                for tab in notebook.all_tabs() {
                    diagnostics::apply_to_tab(&tab, &diags);
                }
            });
        });
    }

    // Run → Clippy
    {
        let st = state.clone();
        menubar::connect_action(app, "clippy", move || {
            let s = st.borrow();
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() { let _ = tab.save(); }
            }
            let cargo_dir = s.project_dir.clone()
                .filter(|pd| pd.join("Cargo.toml").exists())
                .or_else(|| s.notebook.current_tab().and_then(|t| t.path()).and_then(|p| find_cargo_toml_dir(&p)));
            let Some(cd) = cargo_dir else {
                s.output.append_run_error("No Cargo.toml found — open a project first.");
                s.output.show_panel();
                return;
            };
            let notebook  = s.notebook.clone();
            let output    = s.output.clone();
            let last_diag = s.last_diagnostics.clone();
            let cd2 = cd.clone();
            s.runner.run_in_dir_json(&cd, &["clippy"], &s.output, move |json_lines| {
                let diags = diagnostics::parse_cargo_json(&json_lines, &cd2);
                *last_diag.borrow_mut() = diags.clone();
                output.set_diagnostics(&diags);
                for tab in notebook.all_tabs() {
                    diagnostics::apply_to_tab(&tab, &diags);
                }
            });
        });
    }

    // Run → Format File (manual rustfmt trigger)
    {
        let st = state.clone();
        menubar::connect_action(app, "format-file", move || {
            let s = st.borrow();
            let Some(tab) = s.notebook.current_tab() else { return };
            let Some(path) = tab.path() else {
                s.output.append_run_error("Save the file before formatting.");
                return;
            };
            if path.extension().map(|e| e != "rs").unwrap_or(true) {
                s.output.append_run_line("Format File: only .rs files are supported.");
                return;
            }
            if tab.is_modified() { let _ = tab.save(); }
            let output = s.output.clone();
            let diags  = s.last_diagnostics.clone();
            crate::runner::RunManager::rustfmt_file(path, move |result| {
                match result {
                    Ok(()) => {
                        if let Ok(content) = std::fs::read_to_string(tab.path().unwrap()) {
                            let buf = tab.buffer();
                            let cursor_line = buf.iter_at_mark(&buf.get_insert()).line();
                            buf.set_text(&content);
                            buf.set_modified(false);
                            *tab.modified.borrow_mut() = false;
                            if let Some(iter) = buf.iter_at_line(cursor_line) {
                                buf.place_cursor(&iter);
                            }
                            diagnostics::apply_to_tab(&tab, &diags.borrow());
                            output.append_run_success("✓ rustfmt applied.");
                        }
                    }
                    Err(msg) => output.append_run_error(&format!("rustfmt: {}", msg)),
                }
            });
        });
    }

    // Run → Open in Browser
    {
        let st = state.clone();
        menubar::connect_action(app, "open-browser", move || {
            if let Some(tab) = st.borrow().notebook.current_tab() {
                if let Some(path) = tab.path() {
                    RunManager::open_in_browser(&path);
                }
            }
        });
    }

    // ── Codegen: double-click a widget → open companion -app.rs ──
    {
        let st = state.clone();
        state.borrow().canvas.set_on_double_click(move |class, id| {
            // Only generate handlers for interactive widgets that have signals
            if !crate::codegen::has_signal(class) {
                return;
            }
            let s = st.borrow();
            // Need the current .ui file path to derive the companion path
            let ui_path = match s.notebook.current_tab().and_then(|t| t.path()) {
                Some(p) if Canvas::is_ui_file(&p) => p,
                _ => return, // not a .ui tab — nothing to do
            };

            // Get the current XML from the editor buffer
            let xml = match s.notebook.current_tab() {
                Some(tab) => {
                    let (start, end) = tab.buffer().bounds();
                    tab.buffer().text(&start, &end, false).to_string()
                }
                None => return,
            };

            let companion = crate::codegen::companion_path(&ui_path);

            // Read or create the companion file
            let existing = std::fs::read_to_string(&companion).unwrap_or_default();
            let ui_filename = ui_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let new_content = if existing.is_empty() {
                crate::codegen::generate_all_handlers(&xml, &ui_filename)
            } else {
                crate::codegen::merge_handlers(&existing, &xml, &ui_filename)
            };

            // Write the companion file
            if let Err(e) = std::fs::write(&companion, &new_content) {
                log::error!("Failed to write companion file: {}", e);
                return;
            }

            // Open the companion in a new tab (or switch to it if already open)
            s.notebook.open_file(&companion, &s.cfg);

            // Force-refresh the buffer so there's no stale-disk prompt
            if let Some(tab) = s.notebook.current_tab() {
                tab.buffer().set_text(&new_content);
                tab.buffer().set_modified(false);
                *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                    .ok()
                    .and_then(|m| m.modified().ok());
            }

            // Find the handler function for this specific widget and jump to it
            let mut counts = std::collections::BTreeMap::new();
            let fn_name = crate::codegen::make_fn_name_pub(class, id, &mut counts);
            if let Some(tab) = s.notebook.current_tab() {
                if let Some(byte_off) = crate::codegen::find_handler_offset(&new_content, &fn_name) {
                    let char_off = new_content[..byte_off].chars().count();
                    let iter = tab.buffer().iter_at_offset(char_off as i32);
                    tab.buffer().place_cursor(&iter);
                    tab.view.scroll_to_iter(&mut iter.clone(), 0.1, true, 0.0, 0.3);
                }
            }
        });
    }

    // Run → Generate All Handlers
    {
        let st = state.clone();
        menubar::connect_action(app, "generate-handlers", move || {
            let s = st.borrow();

            // Gather XML and optional file path from the current tab.
            // Works for both saved .ui files and unsaved template buffers.
            let (xml, maybe_ui_path) = match s.notebook.current_tab() {
                Some(tab) => {
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    let path = tab.path().filter(|p| Canvas::is_ui_file(p));
                    // If no .ui path but content looks like a UI file, accept it.
                    let looks_like_ui = text.contains("<interface") || text.contains("<object");
                    if path.is_none() && !looks_like_ui {
                        s.output.append_run_error(
                            "Generate Handlers: current tab does not contain .ui XML.",
                        );
                        s.output.show_panel();
                        return;
                    }
                    (text, path)
                }
                None => {
                    s.output.append_run_error(
                        "Generate Handlers: open or create a .ui layout first.",
                    );
                    s.output.show_panel();
                    return;
                }
            };

            // Switch to Code view so the result is visible.
            s.center_nb.set_current_page(Some(1));
            s.left_nb.set_current_page(Some(0));
            s.minimap.set_visible(true);
            s.output.widget.set_visible(true);

            if let Some(ui_path) = maybe_ui_path {
                // ── Saved .ui file: write companion to disk ──────────────────
                let companion = crate::codegen::companion_path(&ui_path);
                let existing = std::fs::read_to_string(&companion).unwrap_or_default();
                let ui_filename = ui_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let new_content = if existing.is_empty() {
                    crate::codegen::generate_all_handlers(&xml, &ui_filename)
                } else {
                    crate::codegen::merge_handlers(&existing, &xml, &ui_filename)
                };

                if let Err(e) = std::fs::write(&companion, &new_content) {
                    s.output.append_run_error(&format!(
                        "Failed to write {}: {}", companion.display(), e
                    ));
                    s.output.show_panel();
                    return;
                }

                s.notebook.open_file(&companion, &s.cfg);

                // Force-refresh the buffer so there's no stale-disk prompt.
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(&new_content);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                        .ok()
                        .and_then(|m| m.modified().ok());
                }

                s.output.append_run_line(&format!(
                    "✓ Generated handlers → {}", companion.display()
                ));
            } else {
                // ── Unsaved template buffer: open generated code in new tab ──
                let new_content = crate::codegen::generate_all_handlers(&xml, "layout.ui");
                s.notebook.new_tab(&s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(&new_content);
                    tab.buffer().set_modified(true); // mark unsaved so Ctrl+S prompts for path
                }
                s.output.append_run_line(
                    "✓ Generated handlers (unsaved) — use Ctrl+S to save as a .rs file.",
                );
            }

            s.output.show_panel();
        });
    }

    // Run → Template Library
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "template-library", move || {
            let s = st.borrow();
            crate::codegen::show_template_library(&win, &s.notebook, &s.cfg);
        });
    }

    // AI → Open AI Chat (swaps to Web AI tab)
    #[cfg(feature = "preview")]
    {
        let st = state.clone();
        menubar::connect_action(app, "ai-open", move || {
            let s = st.borrow();
            s.right_nb.set_current_page(Some(1));
            s.ai_stack.set_visible_child_name("web");
        });
    }
    #[cfg(not(feature = "preview"))]
    menubar::connect_action(app, "ai-open", move || {});

    // AI → Open Claude Code
    {
        let st = state.clone();
        menubar::connect_action(app, "claude-code-open", move || {
            let s = st.borrow();
            s.right_nb.set_current_page(Some(1));
            s.ai_stack.set_visible_child_name("claude");
        });
    }

    // AI → Open API Chat
    {
        let st = state.clone();
        menubar::connect_action(app, "api-chat-open", move || {
            let s = st.borrow();
            s.right_nb.set_current_page(Some(1));
            s.ai_stack.set_visible_child_name("api");
        });
    }

    // Wire Claude Code callbacks.
    {
        let st = state.clone();
        claude_panel.on_get_buffer(move || {
            let s = st.borrow();
            s.notebook.current_tab().map(|tab| {
                let buf = tab.buffer();
                let (start, end) = buf.bounds();
                buf.text(&start, &end, false).to_string()
            })
        });
    }
    {
        let st = state.clone();
        claude_panel.on_get_path(move || {
            let s = st.borrow();
            s.notebook.current_tab()
                .and_then(|tab| tab.path())
                .map(|p| p.display().to_string())
        });
    }
    // Get companion .rs file (path + contents) for the current .ui tab.
    {
        let st = state.clone();
        claude_panel.on_get_companion(move || {
            let s = st.borrow();
            let tab = s.notebook.current_tab()?;
            let ui_path = tab.path().filter(|p| Canvas::is_ui_file(p))?;
            let companion = crate::codegen::companion_path(&ui_path);
            let content = std::fs::read_to_string(&companion).ok()?;
            Some((companion.display().to_string(), content))
        });
    }
    // Apply XML block → replace .ui buffer contents.
    {
        let st = state.clone();
        claude_panel.on_apply_xml(move |code| {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                tab.buffer().set_text(code);
            }
        });
    }
    // Apply Rust block → write to companion .rs file, open it in editor.
    {
        let st = state.clone();
        claude_panel.on_apply_rs(move |code| {
            let s = st.borrow();
            let current_tab = s.notebook.current_tab();
            // Find the .ui tab's companion path.
            let companion = current_tab.as_ref()
                .and_then(|t| t.path())
                .filter(|p| Canvas::is_ui_file(p))
                .map(|p| crate::codegen::companion_path(&p));

            if let Some(companion) = companion {
                // Write the code to the companion .rs file.
                if let Err(e) = std::fs::write(&companion, code) {
                    log::error!("Failed to write companion: {}", e);
                    return;
                }
                s.notebook.open_file(&companion, &s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                        .ok()
                        .and_then(|m| m.modified().ok());
                }
            } else if let Some(tab) = current_tab {
                // Current tab is a .rs file — write code back to it directly.
                if let Some(path) = tab.path() {
                    if let Err(e) = std::fs::write(&path, code) {
                        log::error!("Failed to write {}: {}", path.display(), e);
                        return;
                    }
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok());
                } else {
                    // Unsaved buffer — just update it in place.
                    tab.buffer().set_text(code);
                }
            } else {
                // No tab open — open a new one.
                s.notebook.new_tab(&s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    if let Some(lang) = sourceview5::LanguageManager::default().language("rust") {
                        tab.buffer().set_language(Some(&lang));
                    }
                }
            }
        });
    }

    // Apply diff block → open diff tool with diff pre-loaded.
    {
        let win = window.clone();
        let st = state.clone();
        claude_panel.on_apply_diff(move |diff| {
            let working_dir = st.borrow()
                .notebook
                .current_tab()
                .and_then(|t| t.path())
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            crate::diff_tool::show_diff_dialog_with_diff(&win, working_dir, diff);
        });
    }

    // Wire API Chat panel callbacks (mirrors Claude Code panel).
    {
        let st = state.clone();
        ai_chat_panel.on_get_buffer(move || {
            let s = st.borrow();
            s.notebook.current_tab().map(|tab| {
                let buf = tab.buffer();
                let (start, end) = buf.bounds();
                buf.text(&start, &end, false).to_string()
            })
        });
    }
    {
        let st = state.clone();
        ai_chat_panel.on_get_path(move || {
            let s = st.borrow();
            s.notebook.current_tab()
                .and_then(|tab| tab.path())
                .map(|p| p.display().to_string())
        });
    }
    {
        let st = state.clone();
        ai_chat_panel.on_get_companion(move || {
            let s = st.borrow();
            let tab = s.notebook.current_tab()?;
            let ui_path = tab.path().filter(|p| Canvas::is_ui_file(p))?;
            let companion = crate::codegen::companion_path(&ui_path);
            let content = std::fs::read_to_string(&companion).ok()?;
            Some((companion.display().to_string(), content))
        });
    }
    {
        let st = state.clone();
        ai_chat_panel.on_apply_xml(move |code| {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                tab.buffer().set_text(code);
            }
        });
    }
    {
        let st = state.clone();
        ai_chat_panel.on_apply_rs(move |code| {
            let s = st.borrow();
            let current_tab = s.notebook.current_tab();
            let companion = current_tab.as_ref()
                .and_then(|t| t.path())
                .filter(|p| Canvas::is_ui_file(p))
                .map(|p| crate::codegen::companion_path(&p));

            if let Some(companion) = companion {
                if let Err(e) = std::fs::write(&companion, code) {
                    log::error!("Failed to write companion: {}", e);
                    return;
                }
                s.notebook.open_file(&companion, &s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                        .ok()
                        .and_then(|m| m.modified().ok());
                }
            } else if let Some(tab) = current_tab {
                if let Some(path) = tab.path() {
                    if let Err(e) = std::fs::write(&path, code) {
                        log::error!("Failed to write {}: {}", path.display(), e);
                        return;
                    }
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok());
                } else {
                    tab.buffer().set_text(code);
                }
            } else {
                s.notebook.new_tab(&s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    if let Some(lang) = sourceview5::LanguageManager::default().language("rust") {
                        tab.buffer().set_language(Some(&lang));
                    }
                }
            }
        });
    }

    // Apply diff block → open diff tool (API Chat panel).
    {
        let win = window.clone();
        let st = state.clone();
        ai_chat_panel.on_apply_diff(move |diff| {
            let working_dir = st.borrow()
                .notebook
                .current_tab()
                .and_then(|t| t.path())
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            crate::diff_tool::show_diff_dialog_with_diff(&win, working_dir, diff);
        });
    }

    // ── Wire session history panel ─────────────────────────────────────────
    claude_panel.set_history(chat_history.clone());
    ai_chat_panel.set_history(chat_history.clone());
    {
        let st = state.clone();
        chat_history.on_apply_xml(move |code| {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                tab.buffer().set_text(code);
            }
        });
    }
    {
        let st = state.clone();
        chat_history.on_apply_rs(move |code| {
            let s = st.borrow();
            let current_tab = s.notebook.current_tab();
            let companion = current_tab.as_ref()
                .and_then(|t| t.path())
                .filter(|p| Canvas::is_ui_file(p))
                .map(|p| crate::codegen::companion_path(&p));
            if let Some(companion) = companion {
                if let Err(e) = std::fs::write(&companion, code) {
                    log::error!("Failed to write companion: {}", e);
                    return;
                }
                s.notebook.open_file(&companion, &s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                }
            } else if let Some(tab) = current_tab {
                tab.buffer().set_text(code);
            }
        });
    }
    {
        let win = window.clone();
        let st = state.clone();
        chat_history.on_view_diff(move |diff| {
            let working_dir = st.borrow()
                .notebook
                .current_tab()
                .and_then(|t| t.path())
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            crate::diff_tool::show_diff_dialog_with_diff(&win, working_dir, diff);
        });
    }

    // AI → Copy File for AI
    {
        let st = state.clone();
        menubar::connect_action(app, "ai-copy-file", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                let buf = tab.buffer();
                let (start, end) = buf.bounds();
                let code = buf.text(&start, &end, false).to_string();
                let filename = tab.title();
                let lang = tab.language_name().to_lowercase();
                let lang = map_lang_for_markdown(&lang);
                #[cfg(feature = "preview")]
                let formatted = ai_panel::format_for_ai(&filename, lang, &code, "");
                #[cfg(not(feature = "preview"))]
                let formatted = format_for_ai_simple(&filename, lang, &code);
                copy_to_clipboard(&formatted);
            }
        });
    }

    // AI → Copy Selection for AI
    {
        let st = state.clone();
        menubar::connect_action(app, "ai-copy-selection", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                let buf = tab.buffer();
                let code = buf.selection_bounds()
                    .map(|(a, b)| buf.text(&a, &b, false).to_string())
                    .unwrap_or_else(|| {
                        let (start, end) = buf.bounds();
                        buf.text(&start, &end, false).to_string()
                    });
                let filename = tab.title();
                let lang = tab.language_name().to_lowercase();
                let lang = map_lang_for_markdown(&lang);
                #[cfg(feature = "preview")]
                let formatted = ai_panel::format_for_ai(&filename, lang, &code, "");
                #[cfg(not(feature = "preview"))]
                let formatted = format_for_ai_simple(&filename, lang, &code);
                copy_to_clipboard(&formatted);
            }
        });
    }

    // AI → Apply AI Diff
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "ai-apply-diff", move || {
            let working_dir = st.borrow()
                .notebook
                .current_tab()
                .and_then(|t| t.path())
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            diff_tool::show_diff_dialog(&win, working_dir);
        });
    }

    // Help → Help
    {
        let win = window.clone();
        menubar::connect_action(app, "help", move || {
            help::show_help(&win);
        });
    }

    // Help → About
    {
        // Capture app clone before closure if needed, but we don't need it.
        let win = window.clone();
        menubar::connect_action(app, "about", move || {
            let logo_path = std::env::current_dir()
                .unwrap_or_default()
                .join("data/org.rui.designer.svg");
            
            let mut builder = gtk4::AboutDialog::builder()
                .transient_for(&win)
                .modal(true)
                .program_name("Rui")
                .version("0.3.0")
                .comments("A GTK4 UI designer for developers, made with Rust.")
                .license_type(gtk4::License::MitX11);

            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&logo_path) {
                builder = builder.logo(&texture.upcast::<gtk4::gdk::Paintable>());
            } else {
                builder = builder.logo_icon_name("applications-graphics-symbolic");
            }

            let dialog = builder.build();
            dialog.present();
        });
    }

    // ── Keyboard shortcuts ─────────────────────────────────────────
    app.set_accels_for_action("app.new-tab",       &["<Ctrl>N"]);
    app.set_accels_for_action("app.open",          &["<Ctrl>O"]);
    app.set_accels_for_action("app.save",          &["<Ctrl>S"]);
    app.set_accels_for_action("app.save-as",       &["<Ctrl><Shift>S"]);
    app.set_accels_for_action("app.close-tab",     &["<Ctrl>W"]);
    app.set_accels_for_action("app.find",             &["<Ctrl>F"]);
    app.set_accels_for_action("app.find-replace",    &["<Ctrl>H"]);
    app.set_accels_for_action("app.search-in-files", &["<Ctrl><Shift>F"]);
    app.set_accels_for_action("app.goto-line",     &["<Ctrl>G"]);
    app.set_accels_for_action("app.toggle-sidebar", &["<Ctrl>B"]);
    app.set_accels_for_action("app.toggle-output", &["<Ctrl>J"]);
    app.set_accels_for_action("app.toggle-canvas", &["<Ctrl><Shift>U"]);
    app.set_accels_for_action("app.toggle-toolbox",&["<Ctrl><Shift>T"]);
    app.set_accels_for_action("app.toggle-preview",&["<Ctrl><Shift>P"]);
    app.set_accels_for_action("app.toggle-minimap",&["<Ctrl>M"]);
    app.set_accels_for_action("app.toggle-dark",   &["<Ctrl><Shift>D"]);
    app.set_accels_for_action("app.layout-code",   &["<Ctrl>1"]);
    app.set_accels_for_action("app.layout-designer",&["<Ctrl>2"]);
    app.set_accels_for_action("app.ai-open",        &["<Ctrl><Alt>A"]);
    app.set_accels_for_action("app.ai-copy-file",  &["<Ctrl><Alt>C"]);
    app.set_accels_for_action("app.ai-apply-diff", &["<Ctrl><Alt>D"]);
    app.set_accels_for_action("app.help",           &["F1"]);
    app.set_accels_for_action("app.goto-definition",&["F12"]);
    app.set_accels_for_action("app.run",            &["F5"]);
    app.set_accels_for_action("app.build",         &["<Ctrl><Shift>B"]);
    app.set_accels_for_action("app.stop",          &["<Shift>F5"]);

    // ── Window-level key handler for designer undo/redo ──────────
    // Intercepts Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z in Capture phase so that
    // when in Design mode these are handled here (and propagation stopped),
    // while in Code mode they fall through to the SourceView's built-in undo.
    {
        use gtk4::EventControllerKey;
        use gtk4::gdk::Key;
        use gtk4::gdk::ModifierType;

        let st = state.clone();
        let cnb = center_nb.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);

        key_ctrl.connect_key_pressed(move |_, key, _, mods| {
            let ctrl  = mods.contains(ModifierType::CONTROL_MASK);
            let shift = mods.contains(ModifierType::SHIFT_MASK);

            // Only intercept in Design tab
            if cnb.current_page() != Some(0) {
                return gtk4::glib::Propagation::Proceed;
            }

            if ctrl && !shift && key == Key::z {
                // Designer undo
                let s = st.borrow();
                let xml = s.history.borrow_mut().undo();
                if let Some(xml) = xml {
                    s.history_applying.set(true);
                    if let Some(tab) = s.notebook.current_tab() {
                        let buf = tab.buffer();
                        buf.begin_user_action();
                        let (mut start, mut end) = (buf.start_iter(), buf.end_iter());
                        buf.delete(&mut start, &mut end);
                        buf.insert(&mut buf.end_iter(), &xml);
                        buf.end_user_action();
                    }
                    s.history_applying.set(false);
                }
                return gtk4::glib::Propagation::Stop;
            }

            if !ctrl && !shift && key == Key::Delete {
                // Delete selected widget
                st.borrow().canvas.delete_selected();
                return gtk4::glib::Propagation::Stop;
            }

            if ctrl && (key == Key::y || (shift && key == Key::z)) {
                // Designer redo
                let s = st.borrow();
                let xml = s.history.borrow_mut().redo();
                if let Some(xml) = xml {
                    s.history_applying.set(true);
                    if let Some(tab) = s.notebook.current_tab() {
                        let buf = tab.buffer();
                        buf.begin_user_action();
                        let (mut start, mut end) = (buf.start_iter(), buf.end_iter());
                        buf.delete(&mut start, &mut end);
                        buf.insert(&mut buf.end_iter(), &xml);
                        buf.end_user_action();
                    }
                    s.history_applying.set(false);
                }
                return gtk4::glib::Propagation::Stop;
            }

            gtk4::glib::Propagation::Proceed
        });

        window.add_controller(key_ctrl);
    }

    // ── Save session on close ─────────────────────────────────────
    {
        let st = state.clone();
        window.connect_close_request(move |_| {
            let s = st.borrow();
            s.claude_panel.kill_process();
            let paths: Vec<PathBuf> = s.notebook.all_tabs()
                .iter()
                .filter_map(|t| t.path())
                .collect();
            crate::session::save(&paths, &s.layout);
            gtk4::glib::Propagation::Proceed
        });
    }

    // ── Drag and drop — open dropped files ────────────────────────
    {
        let st = state.clone();
        let drop = gtk4::DropTarget::new(
            gtk4::gio::File::static_type(),
            gtk4::gdk::DragAction::COPY,
        );
        drop.connect_drop(move |_, value, _, _| {
            if let Ok(file) = value.get::<gtk4::gio::File>() {
                if let Some(path) = file.path() {
                    let s = st.borrow();
                    s.notebook.open_file(&path, &s.cfg);
                    return true;
                }
            }
            false
        });
        window.add_controller(drop);
    }

    // ── File watcher — poll for external changes every 3 s ────────
    {
        let st = state.clone();
        let win = window.clone();
        gtk4::glib::timeout_add_seconds_local(3, move || {
            let tabs = st.borrow().notebook.all_tabs();
            for tab in tabs {
                let path = match tab.path() {
                    Some(p) => p,
                    None => continue,
                };
                // Skip tabs that are mid-save — the mtime will be stale
                if tab.saving.get() {
                    continue;
                }
                let current_mtime = match std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                {
                    Some(m) => m,
                    None => continue,
                };
                let stored = tab.last_mtime.borrow().clone();
                if let Some(stored_mtime) = stored {
                    if current_mtime != stored_mtime {
                        *tab.last_mtime.borrow_mut() = Some(current_mtime);
                        let tab_c = tab.clone();
                        let path_c = path.clone();
                        let win_c = win.clone();
                        gtk4::glib::idle_add_local_once(move || {
                            let filename = path_c
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let alert = gtk4::AlertDialog::builder()
                                .message(format!("\"{}\" changed on disk.", filename))
                                .detail("Reload the file from disk?")
                                .buttons(["Reload", "Keep"])
                                .cancel_button(1i32)
                                .default_button(0i32)
                                .build();
                            alert.choose(
                                Some(&win_c),
                                gtk4::gio::Cancellable::NONE,
                                move |result| {
                                    if result == Ok(0) {
                                        let _ = tab_c.load_file(&path_c);
                                    }
                                },
                            );
                        });
                    }
                }
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    // ── Clean up history session dir on exit ──────────────────────
    {
        let st = state.clone();
        app.connect_shutdown(move |_| {
            st.borrow().claude_panel.kill_process();
            st.borrow().history.borrow().cleanup();
        });
    }

    // ── Crash recovery: check for orphaned sessions at startup ────
    {
        let win = window.clone();
        let st = state.clone();
        gtk4::glib::idle_add_local_once(move || {
            let recoveries = crate::history::DesignerHistory::find_recoveries();
            if recoveries.is_empty() { return; }

            // Handle only the most recent orphaned session
            let (session_dir, xml) = recoveries.into_iter().next().unwrap();

            let dialog = gtk4::AlertDialog::builder()
                .message("Recover designer session?")
                .detail("Rui found an unsaved design session from a previous run that may have crashed. Would you like to recover it?")
                .buttons(["Discard", "Recover"])
                .default_button(1i32)
                .cancel_button(0i32)
                .build();

            let st2 = st.clone();
            dialog.choose(Some(&win), gtk4::gio::Cancellable::NONE, move |result| {
                if result == Ok(1) {
                    // Recover: open the XML in a new tab
                    let s = st2.borrow();
                    s.notebook.new_tab(&s.cfg);
                    if let Some(tab) = s.notebook.current_tab() {
                        tab.buffer().set_text(&xml);
                        if let Some(lang) = sourceview5::LanguageManager::default().language("xml") {
                            tab.buffer().set_language(Some(&lang));
                        }
                        s.canvas.connect_buffer(&tab.buffer());
                        s.toolbox.connect_buffer(&tab.buffer());
                        s.outline.connect_buffer(&tab.buffer());
                        s.node_view.connect_buffer(&tab.buffer());
                        s.node_view.set_rs_content("");
                        s.canvas.render(&xml);
                        s.center_nb.set_current_page(Some(0));
                        s.left_nb.set_current_page(Some(1));
                    }
                }
                crate::history::DesignerHistory::discard_recovery(&session_dir);
            });
        });
    }

    window.present();
}

fn map_lang_for_markdown(lang: &str) -> &str {
    match lang {
        "python" | "python3"    => "python",
        "rust"                  => "rust",
        "javascript"            => "javascript",
        "typescript"            => "typescript",
        "html"                  => "html",
        "css"                   => "css",
        "toml"                  => "toml",
        "json"                  => "json",
        "sh" | "shell" | "bash" => "bash",
        "markdown"              => "markdown",
        "xml"                   => "xml",
        "yaml"                  => "yaml",
        "c"                     => "c",
        "cpp" | "c++"           => "cpp",
        _                       => "",
    }
}

fn copy_to_clipboard(text: &str) {
    if let Some(display) = gtk4::gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

#[cfg(not(feature = "preview"))]
fn format_for_ai_simple(filename: &str, lang: &str, code: &str) -> String {
    format!(
        "File: `{filename}`\n\n```{lang}\n{code}\n```\n\n\
         Please respond **only** with a unified git diff so I can apply it with `git apply`.",
    )
}

/// Start (or restart) the LSP client for `dir` and store it in state.
/// Safe to call multiple times — shuts down the old client first.
fn start_lsp(
    state: &Rc<RefCell<AppState>>,
    dir: &std::path::Path,
) {
    // Shutdown previous client if any
    if let Some(ref lsp) = state.borrow().lsp {
        lsp.shutdown();
    }
    let last_diags = state.borrow().last_diagnostics.clone();
    let notebook   = state.borrow().notebook.clone();
    let client = crate::lsp_client::LspClient::start(dir, notebook, last_diags);
    state.borrow_mut().lsp = client;
    if state.borrow().lsp.is_some() {
        log::info!("LSP: started rust-analyzer for {}", dir.display());
    }
}

fn find_cargo_toml_dir(path: &std::path::Path) -> Option<PathBuf> {
    let mut dir = path.parent()?.to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Generate .desktop file, install.sh, and copy app icon into the release build folder.
fn generate_install_files(
    project_dir: &std::path::Path,
    output: &crate::output::OutputPanel,
) -> Result<(), String> {
    // Read project name from Cargo.toml
    let cargo_toml_path = project_dir.join("Cargo.toml");
    let cargo_text = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|e| format!("Cannot read Cargo.toml: {}", e))?;

    let project_name = cargo_text
        .lines()
        .find(|l| l.trim().starts_with("name"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
        .unwrap_or_else(|| {
            project_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "myapp".into())
        });

    // Pretty title: "my_game" → "My Game"
    let pretty_name: String = project_name
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let release_dir = project_dir.join("target").join("release");
    let _ = std::fs::create_dir_all(&release_dir);

    // 1. Copy app icon
    let rui_icon = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("app.png");
    let icon_dest = release_dir.join(format!("{}.png", project_name));

    // Try the compiled-in path first, fall back to common locations
    let icon_src = if rui_icon.exists() {
        rui_icon
    } else {
        // Fallback: look relative to the running binary or known paths
        let fallbacks = [
            std::path::PathBuf::from("/home/brad/Documents/Rui/data/app.png"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("data").join("app.png")))
                .unwrap_or_default(),
        ];
        fallbacks
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("data/app.png"))
    };

    if icon_src.exists() {
        std::fs::copy(&icon_src, &icon_dest)
            .map_err(|e| format!("Failed to copy icon: {}", e))?;
        output.append_run_line(&format!("✓ Copied icon → {}", icon_dest.display()));
    } else {
        output.append_run_line("⚠ Icon data/app.png not found — .desktop will reference it anyway");
    }

    // 2. Generate .desktop file
    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={pretty_name}\n\
         Exec={bin_name}\n\
         Icon={icon_name}\n\
         Terminal=false\n\
         Categories=GTK;Utility;\n\
         Comment={pretty_name} — Built with Rui\n",
        pretty_name = pretty_name,
        bin_name = project_name,
        icon_name = project_name,
    );

    let desktop_path = release_dir.join(format!("{}.desktop", project_name));
    std::fs::write(&desktop_path, &desktop_content)
        .map_err(|e| format!("Failed to write .desktop: {}", e))?;
    output.append_run_line(&format!("✓ Generated {}", desktop_path.display()));

    // 3. Generate install.sh
    let install_script = format!(
        r#"#!/bin/bash
# Install script for {pretty_name}
# Generated by Rui

set -e

BIN_NAME="{bin_name}"
DESKTOP_FILE="{bin_name}.desktop"
ICON_FILE="{bin_name}.png"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

PREFIX="${{PREFIX:-/usr/local}}"
BIN_DIR="$PREFIX/bin"
DESKTOP_DIR="${{XDG_DATA_HOME:-$HOME/.local/share}}/applications"
ICON_DIR="${{XDG_DATA_HOME:-$HOME/.local/share}}/icons/hicolor/256x256/apps"

echo "Installing {pretty_name}..."
echo "  Binary  → $BIN_DIR/$BIN_NAME"
echo "  Desktop → $DESKTOP_DIR/$DESKTOP_FILE"
echo "  Icon    → $ICON_DIR/$ICON_FILE"
echo ""

# Install binary
sudo install -Dm755 "$SCRIPT_DIR/$BIN_NAME" "$BIN_DIR/$BIN_NAME"

# Install .desktop (update Exec and Icon paths)
mkdir -p "$DESKTOP_DIR"
sed -e "s|^Exec=.*|Exec=$BIN_DIR/$BIN_NAME|" \
    -e "s|^Icon=.*|Icon=$ICON_DIR/$ICON_FILE|" \
    "$SCRIPT_DIR/$DESKTOP_FILE" > "$DESKTOP_DIR/$DESKTOP_FILE"

# Install icon
mkdir -p "$ICON_DIR"
cp "$SCRIPT_DIR/$ICON_FILE" "$ICON_DIR/$ICON_FILE"

# Update desktop database
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

echo ""
echo "✓ {pretty_name} installed successfully!"
echo "  Run with: $BIN_NAME"
"#,
        pretty_name = pretty_name,
        bin_name = project_name,
    );

    let install_path = release_dir.join("install.sh");
    std::fs::write(&install_path, &install_script)
        .map_err(|e| format!("Failed to write install.sh: {}", e))?;

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755));
    }

    output.append_run_line(&format!("✓ Generated {}", install_path.display()));
    output.append_run_line("─── Building release... ───");

    Ok(())
}
