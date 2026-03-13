use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, StyleSchemeManager, View};
use gtk4::ScrolledWindow;
use crate::config::EditorConfig;
use std::path::{Path, PathBuf};

/// A single open file tab.
#[derive(Clone)]
pub struct EditorTab {
    pub view:       View,
    pub scroll:     ScrolledWindow,
    pub path:       std::rc::Rc<std::cell::RefCell<Option<PathBuf>>>,
    pub modified:   std::rc::Rc<std::cell::RefCell<bool>>,
    pub last_mtime: std::rc::Rc<std::cell::RefCell<Option<std::time::SystemTime>>>,
    /// Suppresses the file-watcher reload prompt while a save is in flight.
    pub saving:     std::rc::Rc<std::cell::Cell<bool>>,
}

impl EditorTab {
    pub fn new(cfg: &EditorConfig) -> Self {
        let buffer = Buffer::new(None::<&gtk4::TextTagTable>);
        let view = View::with_buffer(&buffer);

        apply_config(&view, cfg);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&view)
            .build();

        let path      = std::rc::Rc::new(std::cell::RefCell::new(None));
        let modified  = std::rc::Rc::new(std::cell::RefCell::new(false));
        let last_mtime = std::rc::Rc::new(std::cell::RefCell::new(None::<std::time::SystemTime>));
        let saving     = std::rc::Rc::new(std::cell::Cell::new(false));

        let modified_clone = modified.clone();
        buffer.connect_modified_changed(move |buf| {
            *modified_clone.borrow_mut() = buf.is_modified();
        });

        Self { view, scroll, path, modified, last_mtime, saving }
    }

    pub fn load_file(&self, path: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let buffer = self.buffer();
        buffer.set_language(detect_language(path).as_ref());
        buffer.set_text(&content);
        buffer.set_modified(false);
        *self.modified.borrow_mut() = false;
        *self.path.borrow_mut() = Some(path.to_path_buf());
        *self.last_mtime.borrow_mut() = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok());
        Ok(())
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = self.path.borrow().clone().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "No path set")
        })?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        // Flag as saving so the file-watcher doesn't prompt "reload?"
        self.saving.set(true);
        let buf = self.buffer();
        let (start, end) = buf.bounds();
        let text = buf.text(&start, &end, false);
        std::fs::write(path, text.as_str())?;
        buf.set_modified(false);
        *self.modified.borrow_mut() = false;
        *self.path.borrow_mut() = Some(path.to_path_buf());
        *self.last_mtime.borrow_mut() = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.saving.set(false);
        Ok(())
    }

    pub fn apply_scheme(&self, scheme_id: &str) {
        let mgr = StyleSchemeManager::default();
        let scheme = if scheme_id.is_empty() {
            None  // clears explicit scheme → GtkSourceView auto-selects for GTK dark/light
        } else {
            mgr.scheme(scheme_id)
        };
        self.buffer().set_style_scheme(scheme.as_ref());
    }

    pub fn buffer(&self) -> Buffer {
        self.view
            .buffer()
            .downcast::<Buffer>()
            .expect("EditorTab buffer is always sourceview5::Buffer")
    }

    pub fn is_modified(&self) -> bool { *self.modified.borrow() }
    pub fn path(&self) -> Option<PathBuf> { self.path.borrow().clone() }

    pub fn title(&self) -> String {
        self.path.borrow().as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    pub fn language_name(&self) -> String {
        self.buffer().language()
            .map(|l| l.name().to_string())
            .unwrap_or_else(|| "Plain Text".to_string())
    }
}

pub fn apply_config(view: &View, cfg: &EditorConfig) {
    view.set_show_line_numbers(cfg.show_line_numbers);
    view.set_highlight_current_line(cfg.highlight_current_line);
    view.set_tab_width(cfg.tab_width);
    view.set_indent_width(-1);
    view.set_insert_spaces_instead_of_tabs(cfg.insert_spaces);
    view.set_auto_indent(true);
    view.set_indent_on_tab(true);
    view.set_smart_home_end(sourceview5::SmartHomeEndType::Before);
    view.set_show_right_margin(false);
    view.set_wrap_mode(if cfg.word_wrap { gtk4::WrapMode::Word } else { gtk4::WrapMode::None });

    if let Ok(buf) = view.buffer().downcast::<Buffer>() {
        buf.set_highlight_matching_brackets(true);
        let mgr = StyleSchemeManager::default();
        if let Some(scheme) = mgr.scheme(&cfg.color_scheme) {
            buf.set_style_scheme(Some(&scheme));
        }
    }
}

fn detect_language(path: &Path) -> Option<sourceview5::Language> {
    let mgr = LanguageManager::default();
    if let Some(lang) = mgr.guess_language(Some(path), None) {
        return Some(lang);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let id = match ext {
        "py"                  => "python",
        "rs"                  => "rust",
        "js" | "mjs" | "cjs"  => "javascript",
        "ts"                  => "typescript",
        "html" | "htm"        => "html",
        "css"                 => "css",
        "toml"                => "toml",
        "json"                => "json",
        "sh" | "bash"         => "sh",
        "md"                  => "markdown",
        "xml" | "ui"          => "xml",
        "yaml" | "yml"        => "yaml",
        "c"                   => "c",
        "cpp" | "cc" | "cxx"  => "cpp",
        _ => return None,
    };
    mgr.language(id)
}
