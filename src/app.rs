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

    #[cfg(feature = "preview")]
    let editor_preview_paned = {
        let paned = Paned::new(Orientation::Horizontal);
        paned.set_start_child(Some(&editor_col));
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
    let editor_preview_paned = editor_col.clone();

    let main_paned = Paned::new(Orientation::Horizontal);
    main_paned.set_start_child(Some(&sidebar.widget));
    main_paned.set_end_child(Some(&editor_preview_paned));
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
        main_paned: main_paned.clone(),
        vert_paned: vert_paned.clone(),
        minimap:   minimap.clone(),
        #[cfg(feature = "preview")]
        preview:   preview.clone(),
        #[cfg(feature = "preview")]
        editor_preview_paned: editor_preview_paned.clone(),
    }));

    // ── Connect notebook tab-switch → statusbar + minimap ─────────
    {
        let st = state.clone();
        notebook.on_switch(move |_, tab| {
            let s = st.borrow();
            s.statusbar.connect_tab(tab);
            s.find_bar.set_buffer(&tab.buffer());
            s.minimap.set_view(&tab.view);
        });
    }

    // ── Connect sidebar file-open → notebook ──────────────────────
    {
        let st = state.clone();
        sidebar.on_open(move |path| {
            let s = st.borrow();
            s.notebook.open_file(&path, &s.cfg);
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
        let s = state.borrow();
        let files_to_open = if open_paths.is_empty() {
            crate::session::load()
        } else {
            open_paths.clone()
        };

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

    // Run → Run
    {
        let st = state.clone();
        menubar::connect_action(app, "run", move || {
            let s = st.borrow();
            if let Some(tab) = s.notebook.current_tab() {
                if tab.path().is_some() {
                    let _ = tab.save();
                }
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
            if let Some(tab) = s.notebook.current_tab() {
                let _ = tab.save();
                if let Some(path) = tab.path() {
                    let output = s.output.clone();
                    let runner = s.runner.clone();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "rs" {
                        let cargo_dir = find_cargo_toml_dir(&path);
                        if let Some(cd) = cargo_dir {
                            output.clear_run();
                            output.show_panel();
                            output.switch_to_run();
                            output.append_run_line("▶ cargo build");
                            let (tx, rx) = async_channel::unbounded::<String>();
                            std::thread::spawn(move || {
                                let mut child = std::process::Command::new("cargo")
                                    .arg("build")
                                    .current_dir(&cd)
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped())
                                    .spawn()
                                    .unwrap();
                                use std::io::BufRead;
                                if let Some(out) = child.stderr.take() {
                                    for line in std::io::BufReader::new(out).lines().flatten() {
                                        let _ = tx.send_blocking(line);
                                    }
                                }
                                let code = child.wait().ok().and_then(|s| s.code());
                                let _ = tx.send_blocking(format!(
                                    "── exit {} ──",
                                    code.unwrap_or(-1)
                                ));
                            });
                            gtk4::glib::spawn_future_local(async move {
                                while let Ok(line) = rx.recv().await {
                                    output.append_run_line(&line);
                                }
                            });
                            return;
                        }
                    }
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
    app.set_accels_for_action("app.toggle-preview",&["<Ctrl><Shift>P"]);
    app.set_accels_for_action("app.toggle-minimap",&["<Ctrl>M"]);
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
            crate::session::save(&paths);
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
