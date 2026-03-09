use sourceview5::{prelude::*, Map};
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, FileDialog, Orientation, Paned,
};
use crate::config::EditorConfig;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::diff_tool;
use crate::find::FindBar;
use crate::goto;
use crate::help;
use crate::menubar;
use crate::canvas::Canvas;
use crate::toolbox::Toolbox;

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
    /// Current layout: "code" or "designer"
    layout:    String,
    /// The project directory (set by Open Project / New Project).
    /// Used by Run/Build to find the correct Cargo.toml.
    project_dir: Option<PathBuf>,

    #[cfg(feature = "preview")]
    preview: PreviewPane,

    main_paned: Paned,
    vert_paned: Paned,
    #[cfg(feature = "preview")]
    editor_preview_paned: Paned,
    minimap: Map,
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
    let toolbox    = Toolbox::new();

    #[cfg(feature = "preview")]
    let preview = PreviewPane::new();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Rui")
        .default_width(1200)
        .default_height(800)
        .build();

    let menubar_widget = menubar::build(app);

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

    // Canvas sits to the right of the editor for .ui design

    let canvas_paned = Paned::new(Orientation::Horizontal);
    canvas_paned.set_start_child(Some(&editor_col));
    canvas_paned.set_end_child(Some(&canvas.widget));
    canvas_paned.set_resize_start_child(true);
    canvas_paned.set_resize_end_child(true);
    canvas_paned.set_shrink_start_child(false);
    canvas_paned.set_shrink_end_child(false);
    // Position is set dynamically on map to get true 50%
    canvas.widget.set_visible(false); // hidden until a .ui file is opened

    // Set canvas pane to 50% once the window is realized
    {
        let cp = canvas_paned.clone();
        canvas_paned.connect_map(move |_| {
            let width = cp.allocated_width();
            if width > 0 {
                cp.set_position(width / 2);
            }
        });
    }

    #[cfg(feature = "preview")]
    let editor_preview_paned = {
        let paned = Paned::new(Orientation::Horizontal);
        paned.set_start_child(Some(&canvas_paned));
        paned.set_end_child(Some(&preview.widget));
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_position(800);
        if !cfg.show_preview {
            preview.widget.set_visible(false);
        }
        paned
    };

    #[cfg(not(feature = "preview"))]
    let editor_preview_paned = canvas_paned.clone();

    let main_paned = Paned::new(Orientation::Horizontal);
    main_paned.set_start_child(Some(&sidebar.widget));

    // Toolbox sits between sidebar and editor (palette + inspector stacked)
    let toolbox_editor_box = GtkBox::new(Orientation::Horizontal, 0);
    toolbox_editor_box.append(&toolbox.widget);
    toolbox_editor_box.append(&editor_preview_paned);
    toolbox.widget.set_visible(false); // hidden until toggled

    main_paned.set_end_child(Some(&toolbox_editor_box));
    main_paned.set_resize_start_child(false);
    main_paned.set_resize_end_child(true);
    main_paned.set_position(210);
    if !cfg.show_sidebar {
        sidebar.widget.set_visible(false);
    }

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
        layout:    "code".into(),
        project_dir: None,
        main_paned: main_paned.clone(),
        vert_paned: vert_paned.clone(),
        minimap:   minimap.clone(),
        #[cfg(feature = "preview")]
        preview:   preview.clone(),
        #[cfg(feature = "preview")]
        editor_preview_paned: editor_preview_paned.clone(),
    }));

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

            // Show canvas and connect toolbox if this is a .ui file
            if let Some(path) = tab.path() {
                if Canvas::is_ui_file(&path) {
                    s.canvas.widget.set_visible(true);
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
                } else {
                    s.canvas.widget.set_visible(false);
                    s.canvas.clear();
                }
            } else {
                s.canvas.widget.set_visible(false);
                s.canvas.clear();
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
            // Activate canvas + toolbox if this is a .ui file
            if Canvas::is_ui_file(&path) {
                if let Some(tab) = s.notebook.current_tab() {
                    s.canvas.widget.set_visible(true);
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
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
            s.canvas.widget.set_visible(true);
            s.toolbox.widget.set_visible(true);
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
            // If the last opened file is a .ui, activate canvas + toolbox
            if let Some(path) = tab.path() {
                if Canvas::is_ui_file(&path) {
                    s.canvas.widget.set_visible(true);
                    s.canvas.connect_buffer(&tab.buffer());
                    s.toolbox.connect_buffer(&tab.buffer());
                    let (start, end) = tab.buffer().bounds();
                    let text = tab.buffer().text(&start, &end, false).to_string();
                    s.canvas.render(&text);
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
        menubar::connect_action(app, "new-ui", move || {
            let s = st.borrow();
            s.notebook.new_tab(&s.cfg);
            if let Some(tab) = s.notebook.current_tab() {
                let template = r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="main_box">
    <property name="orientation">vertical</property>
    <property name="spacing">8</property>
    <property name="margin-start">12</property>
    <property name="margin-end">12</property>
    <property name="margin-top">12</property>
    <property name="margin-bottom">12</property>
    <child>
      <!-- Add widgets here -->
    </child>
  </object>
</interface>
"#;
                tab.buffer().set_text(template);
                // Set XML language for syntax highlighting
                if let Some(lang) = sourceview5::LanguageManager::default()
                    .language("xml")
                {
                    tab.buffer().set_language(Some(&lang));
                }
                tab.buffer().set_modified(false);
                s.toolbox.set_buffer(&tab.buffer());
                s.canvas.widget.set_visible(true);
                s.canvas.connect_buffer(&tab.buffer());
                s.toolbox.connect_buffer(&tab.buffer());
                s.canvas.render(template);
            }
        });
    }

    // File → New Project
    {
        let st = state.clone();
        let win = window.clone();
        menubar::connect_action(app, "new-project", move || {
            let st2 = st.clone();
            let dialog = FileDialog::builder()
                .title("New Project — Select or Create a Folder")
                .modal(true)
                .build();
            dialog.select_folder(Some(&win), gtk4::gio::Cancellable::NONE, move |result| {
                if let Ok(folder) = result {
                    if let Some(dir) = folder.path() {
                        // Derive project name from folder name
                        let project_name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "my_app".into())
                            .replace(' ', "_")
                            .to_lowercase();

                        if let Err(e) = crate::codegen::scaffold_project(&dir, &project_name) {
                            log::error!("Failed to scaffold project: {}", e);
                            return;
                        }

                        // Remember project directory for Run/Build
                        st2.borrow_mut().project_dir = Some(dir.clone());

                        let s = st2.borrow();

                        // Point sidebar at the new project
                        s.sidebar.set_root(&dir);

                        // Open layout.ui in the editor
                        let ui_path = dir.join("layout.ui");
                        s.notebook.open_file(&ui_path, &s.cfg);

                        // Activate canvas + toolbox for the .ui file
                        if let Some(tab) = s.notebook.current_tab() {
                            s.toolbox.set_buffer(&tab.buffer());
                            s.canvas.widget.set_visible(true);
                            s.toolbox.widget.set_visible(true);
                            s.canvas.connect_buffer(&tab.buffer());
                            s.toolbox.connect_buffer(&tab.buffer());
                            let (start, end) = tab.buffer().bounds();
                            let text = tab.buffer().text(&start, &end, false).to_string();
                            s.canvas.render(&text);
                        }

                        // Also open main.rs so they can see the bootstrap
                        let main_path = dir.join("src").join("main.rs");
                        s.notebook.open_file(&main_path, &s.cfg);

                        // Switch back to the .ui tab
                        s.notebook.open_file(&ui_path, &s.cfg);

                        s.output.append_run_line(&format!(
                            "✓ Created project \"{}\" → {}",
                            project_name,
                            dir.display()
                        ));
                        s.output.show_panel();
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
                        let s = st2.borrow();
                        s.notebook.open_file(&path, &s.cfg);
                        // Always update toolbox buffer for the new tab
                        if let Some(tab) = s.notebook.current_tab() {
                            s.toolbox.set_buffer(&tab.buffer());
                        }
                        // Activate canvas + toolbox if this is a .ui file
                        if Canvas::is_ui_file(&path) {
                            if let Some(tab) = s.notebook.current_tab() {
                                s.canvas.widget.set_visible(true);
                                s.canvas.connect_buffer(&tab.buffer());
                                s.toolbox.connect_buffer(&tab.buffer());
                                let (start, end) = tab.buffer().bounds();
                                let text = tab.buffer().text(&start, &end, false).to_string();
                                s.canvas.render(&text);
                            }
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

                        let s = st2.borrow();
                        s.sidebar.set_root(&dir);

                        // Auto-open layout.ui if it exists
                        let ui_path = dir.join("layout.ui");
                        if ui_path.exists() {
                            s.notebook.open_file(&ui_path, &s.cfg);
                            if let Some(tab) = s.notebook.current_tab() {
                                s.toolbox.set_buffer(&tab.buffer());
                                s.canvas.widget.set_visible(true);
                                s.toolbox.widget.set_visible(true);
                                s.canvas.connect_buffer(&tab.buffer());
                                s.toolbox.connect_buffer(&tab.buffer());
                                let (start, end) = tab.buffer().bounds();
                                let text = tab.buffer().text(&start, &end, false).to_string();
                                s.canvas.render(&text);
                            }
                        }

                        s.output.append_run_line(&format!(
                            "✓ Opened project → {}",
                            dir.display()
                        ));
                        s.output.show_panel();
                    }
                }
            });
        });
    }

    // File → Save
    {
        let st = state.clone();
        menubar::connect_action(app, "save", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                if tab.path().is_some() {
                    let _ = s.notebook.save_current();
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
                        Ok(()) => saved += 1,
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
            let vis = s.sidebar.widget.is_visible();
            s.sidebar.widget.set_visible(!vis);
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
            st.borrow().canvas.toggle();
        });
    }

    // View → Toggle Toolbox (palette + inspector)
    {
        let st = state.clone();
        menubar::connect_action(app, "toggle-toolbox", move || {
            st.borrow().toolbox.toggle();
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
            s.sidebar.widget.set_visible(true);
            s.main_paned.set_position(210);
            s.canvas.widget.set_visible(false);
            s.toolbox.widget.set_visible(false);
            #[cfg(feature = "preview")]
            s.preview.widget.set_visible(false);
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
            s.sidebar.widget.set_visible(true);
            s.main_paned.set_position(210);
            s.canvas.widget.set_visible(true);
            s.toolbox.widget.set_visible(true);
            #[cfg(feature = "preview")]
            s.preview.widget.set_visible(false);
            s.minimap.set_visible(false);
            s.output.widget.set_visible(true);
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
            let ui_path = match s.notebook.current_tab().and_then(|t| t.path()) {
                Some(p) if Canvas::is_ui_file(&p) => p,
                _ => {
                    s.output.append_run_error(
                        "Generate Handlers: open a .ui file first.",
                    );
                    s.output.show_panel();
                    return;
                }
            };

            let xml = {
                let tab = s.notebook.current_tab().unwrap();
                let (start, end) = tab.buffer().bounds();
                tab.buffer().text(&start, &end, false).to_string()
            };

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
                    "Failed to write {}: {}",
                    companion.display(),
                    e
                ));
                s.output.show_panel();
                return;
            }

            s.notebook.open_file(&companion, &s.cfg);

            // Force-refresh the buffer so there's no stale-disk prompt
            if let Some(tab) = s.notebook.current_tab() {
                tab.buffer().set_text(&new_content);
                tab.buffer().set_modified(false);
                *tab.last_mtime.borrow_mut() = std::fs::metadata(&companion)
                    .ok()
                    .and_then(|m| m.modified().ok());
            }

            s.output.append_run_line(&format!(
                "✓ Generated handlers → {}",
                companion.display()
            ));
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

    // AI → Open AI Chat
    #[cfg(feature = "preview")]
    {
        let win = window.clone();
        menubar::connect_action(app, "ai-open", move || {
            ai_panel::show_ai_panel(&win);
        });
    }
    #[cfg(not(feature = "preview"))]
    menubar::connect_action(app, "ai-open", move || {});

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

    // ── Save session on close ─────────────────────────────────────
    {
        let st = state.clone();
        window.connect_close_request(move |_| {
            let s = st.borrow();
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
