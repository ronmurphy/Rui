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
use crate::find::FindBar;
use crate::goto;
use crate::help;
use crate::menubar;
use crate::canvas::Canvas;
use crate::claude_code::ClaudeCodePanel;
use crate::outline::OutlinePanel;
use crate::toolbox::Toolbox;

#[cfg(feature = "preview")]
use crate::ai_panel;
use crate::notebook::NotebookManager;
use crate::output::OutputPanel;
use crate::runner::RunManager;
use crate::sidebar::FileTree;
use crate::statusbar::StatusBar;

/// Resize a window that is already visible (set_default_size only works before show).
fn resize_window(win: &ApplicationWindow, w: i32, h: i32) {
    use gtk4::prelude::*;
    // In GTK4 on Wayland/X11 the surface-level request is the correct way.
    if let Some(surface) = win.surface() {
        if let Some(toplevel) = surface.downcast_ref::<gtk4::gdk::ToplevelLayout>() {
            // This path won't work — ToplevelLayout isn't the surface.
            let _ = toplevel;
        }
    }
    // The simplest GTK4 approach: set_default_size + queue_resize.
    win.set_default_size(w, h);
}

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
    /// Dynamic gio::Menu backing the File → Recent Files submenu.
    recent_files_menu: gtk4::gio::Menu,
    /// Dynamic gio::Menu backing the File → Recent Projects submenu.
    recent_projects_menu: gtk4::gio::Menu,

    #[cfg(feature = "preview")]
    preview: PreviewPane,

    #[cfg(feature = "preview")]
    ai_sidebar: crate::ai_panel::AiSidebar,

    claude_panel: ClaudeCodePanel,

    main_paned: Paned,
    vert_paned: Paned,
    left_nb:   GtkNotebook,
    center_nb: GtkNotebook,
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

    let notebook   = NotebookManager::new();
    let sidebar    = FileTree::new();
    let output     = OutputPanel::new();
    let statusbar  = StatusBar::new();
    let runner     = RunManager::new();
    let find_bar   = FindBar::new();
    let canvas     = Canvas::new();
    canvas.init_merge_toolbar();
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

    let (menubar_widget, recent_files_menu, recent_projects_menu) = menubar::build(app);

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

    // ── Center notebook: Design (canvas) | Code (editor) ──────────
    let center_nb = GtkNotebook::new();
    center_nb.set_show_border(false);
    center_nb.set_hexpand(true);
    center_nb.set_vexpand(true);
    canvas.widget.set_hexpand(true);
    canvas.widget.set_vexpand(true);
    center_nb.append_page(&canvas.widget,  Some(&Label::new(Some("  Design  "))));
    center_nb.append_page(&editor_col,     Some(&Label::new(Some("  Code  "))));
    center_nb.set_current_page(Some(1)); // start on Code tab

    // Re-render the canvas whenever the Design tab becomes visible.
    // This handles the case where the user was on the Code tab (possibly
    // switching editor tabs to a non-.ui file which cleared the canvas),
    // then switches back to the Design tab.
    {
        let canvas_ref = canvas.clone();
        center_nb.connect_switch_page(move |_, _, page| {
            if page == 0 {
                canvas_ref.render_from_buffer();
            }
        });
    }

    // ── Left notebook: Files (sidebar) | Widgets (toolbox) ────────
    let left_nb = GtkNotebook::new();
    left_nb.set_show_border(false);
    left_nb.set_width_request(230);
    sidebar.widget.set_visible(true);
    toolbox.widget.set_visible(true);
    outline.widget.set_visible(true);
    left_nb.append_page(&sidebar.widget,  Some(&Label::new(Some("  Files  "))));
    left_nb.append_page(&toolbox.widget,  Some(&Label::new(Some("  Widgets  "))));
    left_nb.append_page(&outline.widget,  Some(&Label::new(Some("  Tree  "))));
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

    // Wrap center_nb + AI sidebar + Claude panel in a horizontal box so
    // sidebars appear to the right of the designer/code area.
    let center_area = GtkBox::new(Orientation::Horizontal, 0);
    center_area.set_hexpand(true);
    center_area.set_vexpand(true);
    center_area.append(&center_nb);
    #[cfg(feature = "preview")]
    center_area.append(&ai_sidebar.widget);
    center_area.append(&claude_panel.widget);

    let main_paned = Paned::new(Orientation::Horizontal);
    main_paned.set_start_child(Some(&left_nb));
    main_paned.set_end_child(Some(&center_area));
    main_paned.set_resize_start_child(false);
    main_paned.set_resize_end_child(true);
    main_paned.set_shrink_start_child(false);
    main_paned.set_shrink_end_child(false);
    main_paned.set_position(230);

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
    root.append(&menubar_widget);
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
        layout:    "code".into(),
        project_dir: None,
        history: Rc::new(RefCell::new(crate::history::DesignerHistory::new())),
        history_applying: Rc::new(Cell::new(false)),
        mru: crate::mru::Mru::load(),
        recent_files_menu: recent_files_menu.clone(),
        recent_projects_menu: recent_projects_menu.clone(),
        main_paned: main_paned.clone(),
        vert_paned: vert_paned.clone(),
        left_nb:   left_nb.clone(),
        center_nb: center_nb.clone(),
        minimap:   minimap.clone(),
        #[cfg(feature = "preview")]
        preview:   preview.clone(),
        #[cfg(feature = "preview")]
        ai_sidebar: ai_sidebar.clone(),
        claude_panel: claude_panel.clone(),
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

    // Wire outline rebuild → toolbox selector combo.
    {
        let toolbox_ref = toolbox.clone();
        outline.on_rebuild(move |items| {
            toolbox_ref.populate_selector(items);
        });
    }

    // Wire toolbox selector combo selection → green canvas highlight.
    {
        let canvas_ref = canvas.clone();
        toolbox.on_select_widget(move |byte_offset| {
            canvas_ref.select_from_tree(byte_offset);
        });
    }

    // Wire toolbox selector trash button → delete widget from buffer.
    {
        let outline_ref = outline.clone();
        toolbox.on_delete_widget(move |start, end| {
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

    {
        let st = state.clone();
        notebook.on_switch(move |_, tab| {
            let s = st.borrow();
            s.statusbar.connect_tab(tab);
            s.find_bar.set_buffer(&tab.buffer());
            s.minimap.set_view(&tab.view);
            s.toolbox.set_buffer(&tab.buffer());

            // Connect canvas, toolbox, and outline if this is a .ui file
            if let Some(path) = tab.path() {
                if Canvas::is_ui_file(&path) {
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
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
                }
            } else {
                s.canvas.clear();
                s.outline.clear_buffer();
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
            // Activate canvas + toolbox + outline if this is a .ui file
            if Canvas::is_ui_file(&path) {
                if let Some(tab) = s.notebook.current_tab() {
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    s.outline.connect_buffer(&tab.buffer());
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

                            let s = st3.borrow();
                            s.sidebar.set_root(&dir);

                            let ui_path = dir.join("layout.ui");
                            s.notebook.open_file(&ui_path, &s.cfg);

                            if let Some(tab) = s.notebook.current_tab() {
                                s.toolbox.set_buffer(&tab.buffer());
                                s.canvas.connect_buffer(&tab.buffer());
                                s.toolbox.connect_buffer(&tab.buffer());
                                s.outline.connect_buffer(&tab.buffer());
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

                        {
                            let s = st2.borrow();
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
                                    let (start, end) = tab.buffer().bounds();
                                    let text = tab.buffer().text(&start, &end, false).to_string();
                                    s.canvas.render(&text);
                                    s.center_nb.set_current_page(Some(0));
                                    s.left_nb.set_current_page(Some(1));
                                }
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
                if tab.path().is_some() {
                    let _ = s.notebook.save_current();
                    validate_ui_tab(&tab, &s.output);
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
            let mut saved = 0usize;
            let mut failed = 0usize;
            for tab in s.notebook.all_tabs() {
                if tab.path().is_some() && tab.is_modified() {
                    match tab.save() {
                        Ok(()) => {
                            saved += 1;
                            validate_ui_tab(&tab, &s.output);
                        }
                        Err(e) => {
                            log::error!("Save All: {}", e);
                            failed += 1;
                        }
                    }
                }
            }
            if failed > 0 {
                s.output.append_run_error(&format!(
                    "Save All: {} saved, {} failed", saved, failed
                ));
            } else if saved > 0 {
                s.output.append_run_line(&format!(
                    "✓ Saved {} file{}", saved, if saved == 1 { "" } else { "s" }
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
            st.borrow().notebook.close_current();
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

    // Edit → Designer Undo (Design mode only; Ctrl+Z)
    {
        let st = state.clone();
        menubar::connect_action(app, "designer-undo", move || {
            let s = st.borrow();
            if s.center_nb.current_page() != Some(0) { return; }
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
        });
    }

    // Edit → Designer Redo (Design mode only; Ctrl+Y / Ctrl+Shift+Z)
    {
        let st = state.clone();
        menubar::connect_action(app, "designer-redo", move || {
            let s = st.borrow();
            if s.center_nb.current_page() != Some(0) { return; }
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
        menubar::connect_action(app, "toggle-dark", move || {
            if let Some(settings) = gtk4::Settings::default() {
                let dark = settings.is_gtk_application_prefer_dark_theme();
                settings.set_gtk_application_prefer_dark_theme(!dark);
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
                s.runner.run_in_dir(&cd, &["build"], &s.output);
            } else if let Some(tab) = s.notebook.current_tab() {
                if let Some(path) = tab.path() {
                    let output = s.output.clone();
                    let runner = s.runner.clone();
                    runner.run_file(&path, &output, || {}, |_| {});
                }
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

    // AI → Open AI Chat (toggle sidebar, resize window to fit)
    #[cfg(feature = "preview")]
    {
        let st = state.clone();
        menubar::connect_action(app, "ai-open", move || {
            let s = st.borrow();
            let now_visible = s.ai_sidebar.toggle();
            let win = &s.window;
            let cur_w = win.width();
            let cur_h = win.height();
            let delta = ai_panel::SIDEBAR_WIDTH;
            if now_visible {
                win.set_default_size(cur_w + delta, cur_h);
            } else {
                win.set_default_size((cur_w - delta).max(800), cur_h);
            }
        });
    }
    #[cfg(not(feature = "preview"))]
    menubar::connect_action(app, "ai-open", move || {});

    // AI → Open Claude Code (toggle panel, resize window)
    {
        let st = state.clone();
        menubar::connect_action(app, "claude-code-open", move || {
            let s = st.borrow();
            let will_show = !s.claude_panel.is_visible();
            let win = &s.window;
            let delta = crate::claude_code::SIDEBAR_WIDTH;
            if will_show {
                // Extend the window first, then reveal the panel on the next frame.
                resize_window(win, win.width() + delta, win.height());
                let widget = s.claude_panel.widget.clone();
                let input = s.claude_panel.input_widget().clone();
                gtk4::glib::idle_add_local_once(move || {
                    widget.set_visible(true);
                    input.grab_focus();
                });
            } else {
                // Hide the panel first, then shrink.
                s.claude_panel.set_visible(false);
                let w = win.clone();
                let cur_w = win.width();
                let cur_h = win.height();
                gtk4::glib::idle_add_local_once(move || {
                    resize_window(&w, (cur_w - delta).max(800), cur_h);
                });
            }
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
            // Find the .ui tab's companion path.
            let companion = s.notebook.current_tab()
                .and_then(|t| t.path())
                .filter(|p| Canvas::is_ui_file(p))
                .map(|p| crate::codegen::companion_path(&p));

            if let Some(companion) = companion {
                // Write the code to disk.
                if let Err(e) = std::fs::write(&companion, code) {
                    log::error!("Failed to write companion: {}", e);
                    return;
                }
                // Open (or switch to) the companion tab and refresh its buffer.
                s.notebook.open_file(&companion, &s.cfg);
                if let Some(tab) = s.notebook.current_tab() {
                    tab.buffer().set_text(code);
                    tab.buffer().set_modified(false);
                    *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                        .ok()
                        .and_then(|m| m.modified().ok());
                }
            } else {
                // No .ui file open — just open a new tab with the rust code.
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
        let win = window.clone();
        menubar::connect_action(app, "about", move || {
            let dialog = gtk4::AboutDialog::builder()
                .transient_for(&win)
                .modal(true)
                .program_name("Rui")
                .version("0.1.0")
                .comments("A GTK4 UI designer for Rust developers.")
                .license_type(gtk4::License::MitX11)
                .build();
            dialog.present();
        });
    }

    // ── Keyboard shortcuts ─────────────────────────────────────────
    app.set_accels_for_action("app.new-tab",       &["<Ctrl>N"]);
    app.set_accels_for_action("app.open",          &["<Ctrl>O"]);
    app.set_accels_for_action("app.save",          &["<Ctrl>S"]);
    app.set_accels_for_action("app.save-as",       &["<Ctrl><Shift>S"]);
    app.set_accels_for_action("app.close-tab",     &["<Ctrl>W"]);
    app.set_accels_for_action("app.find",          &["<Ctrl>F"]);
    app.set_accels_for_action("app.find-replace",  &["<Ctrl>H"]);
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
    app.set_accels_for_action("app.help",          &["F1"]);
    app.set_accels_for_action("app.run",           &["F5"]);
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
