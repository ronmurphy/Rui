use sourceview5::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Notebook, Orientation};
use crate::config::EditorConfig;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::tab::EditorTab;

#[derive(Clone)]
pub struct NotebookManager {
    pub widget: Notebook,
    tabs: Rc<RefCell<Vec<EditorTab>>>,
    on_switch: Rc<RefCell<Option<Box<dyn Fn(usize, &EditorTab)>>>>,
    on_modified: Rc<RefCell<Option<Box<dyn Fn(usize, bool)>>>>,
}

impl NotebookManager {
    pub fn new() -> Self {
        let notebook = Notebook::builder()
            .scrollable(true)
            .show_border(false)
            .hexpand(true)
            .vexpand(true)
            .build();

        let mgr = Self {
            widget: notebook,
            tabs: Rc::new(RefCell::new(Vec::new())),
            on_switch: Rc::new(RefCell::new(None)),
            on_modified: Rc::new(RefCell::new(None)),
        };

        let tabs = mgr.tabs.clone();
        let on_switch = mgr.on_switch.clone();
        mgr.widget.connect_switch_page(move |_, _, page_num| {
            let tabs_ref = tabs.borrow();
            if let Some(tab) = tabs_ref.get(page_num as usize) {
                if let Some(cb) = on_switch.borrow().as_ref() {
                    cb(page_num as usize, tab);
                }
            }
        });

        mgr
    }

    pub fn on_switch<F: Fn(usize, &EditorTab) + 'static>(&self, cb: F) {
        *self.on_switch.borrow_mut() = Some(Box::new(cb));
    }

    pub fn on_modified<F: Fn(usize, bool) + 'static>(&self, cb: F) {
        *self.on_modified.borrow_mut() = Some(Box::new(cb));
    }

    pub fn open_file(&self, path: &Path, cfg: &EditorConfig) {
        let already = {
            let tabs = self.tabs.borrow();
            tabs.iter().position(|t| t.path().as_deref() == Some(path))
        };
        if let Some(idx) = already {
            self.widget.set_current_page(Some(idx as u32));
            return;
        }

        let tab = EditorTab::new(cfg);
        if let Err(e) = tab.load_file(path) {
            log::error!("Failed to load {}: {}", path.display(), e);
            return;
        }
        tab.apply_scheme(&cfg.color_scheme);
        self.add_tab(tab, cfg);
    }

    pub fn new_tab(&self, cfg: &EditorConfig) {
        let tab = EditorTab::new(cfg);
        tab.apply_scheme(&cfg.color_scheme);
        self.add_tab(tab, cfg);
    }

    pub fn save_current(&self) -> std::io::Result<()> {
        if let Some(tab) = self.current_tab() {
            tab.save()
        } else {
            Ok(())
        }
    }

    pub fn save_current_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(tab) = self.current_tab() {
            tab.save_to(path)?;
            if let Some(idx) = self.current_index() {
                self.refresh_label(idx);
            }
        }
        Ok(())
    }

    pub fn close_current(&self) {
        if let Some(idx) = self.current_index() {
            self.close_tab(idx);
        }
    }

    pub fn current_tab(&self) -> Option<EditorTab> {
        let idx = self.current_index()?;
        self.tabs.borrow().get(idx).cloned()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.widget.current_page().map(|p| p as usize)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.borrow().len()
    }

    pub fn all_tabs(&self) -> Vec<EditorTab> {
        self.tabs.borrow().clone()
    }

    fn add_tab(&self, tab: EditorTab, cfg: &EditorConfig) {
        let label_widget = self.build_label(&tab);

        let page_idx = self.widget.append_page(&tab.scroll, Some(&label_widget));
        self.widget.set_tab_reorderable(&tab.scroll, true);

        {
            let mut tabs = self.tabs.borrow_mut();
            tabs.push(tab.clone());
        }

        let tabs_rc = self.tabs.clone();
        let on_modified = self.on_modified.clone();
        let notebook = self.widget.clone();
        let idx = page_idx as usize;
        tab.buffer().connect_modified_changed(move |buf| {
            let is_mod = buf.is_modified();
            if let Some(t) = tabs_rc.borrow().get(idx) {
                *t.modified.borrow_mut() = is_mod;
            }
            if let Some(page_child) = notebook.nth_page(Some(page_idx)) {
                if let Some(lw) = notebook.tab_label(&page_child) {
                    update_label_modified(&lw, is_mod);
                }
            }
            if let Some(cb) = on_modified.borrow().as_ref() {
                cb(idx, is_mod);
            }
        });

        if cfg.autosave {
            let tab_clone = tab.clone();
            let fc = gtk4::EventControllerFocus::new();
            let tc = tab_clone.clone();
            fc.connect_leave(move |_| { let _ = tc.save(); });
            tab.view.add_controller(fc);
        }

        self.widget.set_current_page(Some(page_idx));
    }

    fn close_tab(&self, idx: usize) {
        // Remove from the Vec first, then from the GTK widget.
        // Reversing the order would cause remove_page() to fire switch_page
        // synchronously while borrow_mut() is still held → double-borrow panic.
        {
            let mut tabs = self.tabs.borrow_mut();
            if idx >= tabs.len() { return; }
            tabs.remove(idx);
        } // borrow_mut dropped before remove_page fires switch_page
        self.widget.remove_page(Some(idx as u32));
    }

    fn refresh_label(&self, idx: usize) {
        if let Some(page) = self.widget.nth_page(Some(idx as u32)) {
            if let Some(lw) = self.widget.tab_label(&page) {
                let tabs = self.tabs.borrow();
                if let Some(tab) = tabs.get(idx) {
                    if let Some(hbox) = lw.downcast_ref::<GtkBox>() {
                        let mut child = hbox.first_child();
                        while let Some(w) = child {
                            if let Some(lbl) = w.downcast_ref::<Label>() {
                                if lbl.has_css_class("editor-tab-label") {
                                    lbl.set_text(&tab.title());
                                }
                            }
                            child = w.next_sibling();
                        }
                    }
                }
            }
        }
    }

    fn build_label(&self, tab: &EditorTab) -> GtkBox {
        let hbox = GtkBox::new(Orientation::Horizontal, 2);

        let title_lbl = Label::new(Some(&tab.title()));
        title_lbl.add_css_class("editor-tab-label");
        hbox.append(&title_lbl);

        let close_btn = Button::with_label("×");
        close_btn.add_css_class("editor-tab-close");
        close_btn.set_tooltip_text(Some("Close tab"));

        let tabs_rc = self.tabs.clone();
        let notebook = self.widget.clone();
        let scroll = tab.scroll.clone();
        close_btn.connect_clicked(move |_| {
            if let Some(page_num) = notebook.page_num(&scroll) {
                notebook.remove_page(Some(page_num));
                let mut tabs = tabs_rc.borrow_mut();
                if (page_num as usize) < tabs.len() {
                    tabs.remove(page_num as usize);
                }
            }
        });

        hbox.append(&close_btn);
        hbox.show();
        hbox
    }
}

fn update_label_modified(label_widget: &gtk4::Widget, is_modified: bool) {
    if let Some(hbox) = label_widget.downcast_ref::<GtkBox>() {
        let mut child = hbox.first_child();
        while let Some(w) = child {
            if let Some(lbl) = w.downcast_ref::<Label>() {
                if lbl.has_css_class("editor-tab-label") {
                    if is_modified {
                        lbl.add_css_class("editor-tab-modified");
                    } else {
                        lbl.remove_css_class("editor-tab-modified");
                    }
                }
            }
            child = w.next_sibling();
        }
    }
}
