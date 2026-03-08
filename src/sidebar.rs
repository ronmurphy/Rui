#[allow(deprecated)]

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    CellRendererText, ScrolledWindow, SelectionMode, TreeIter, TreeStore,
    TreeView, TreeViewColumn,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const COL_NAME: u32 = 0;
const COL_PATH: u32 = 1;
const COL_IS_DIR: u32 = 2;

#[allow(deprecated)]
fn col_string(store: &TreeStore, iter: &TreeIter, col: u32) -> String {
    gtk4::prelude::TreeModelExtManual::get::<String>(store, iter, col as i32)
}

#[allow(deprecated)]
fn col_bool(store: &TreeStore, iter: &TreeIter, col: u32) -> bool {
    gtk4::prelude::TreeModelExtManual::get::<bool>(store, iter, col as i32)
}

#[derive(Clone)]
pub struct FileTree {
    pub widget: ScrolledWindow,
    store:      TreeStore,
    tree_view:  TreeView,
    root:       Rc<RefCell<Option<PathBuf>>>,
    on_open:    Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>>,
}

impl FileTree {
    pub fn new() -> Self {
        let store = TreeStore::new(&[
            glib::Type::STRING,
            glib::Type::STRING,
            glib::Type::BOOL,
        ]);

        let tree_view = TreeView::builder()
            .model(&store)
            .headers_visible(false)
            .activate_on_single_click(false)
            .build();

        tree_view.selection().set_mode(SelectionMode::Single);

        let col = TreeViewColumn::new();
        let cell = CellRendererText::new();
        col.pack_start(&cell, true);
        col.add_attribute(&cell, "text", COL_NAME as i32);
        tree_view.append_column(&col);

        let scroll = ScrolledWindow::builder()
            .child(&tree_view)
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(false)
            .vexpand(true)
            .width_request(200)
            .build();
        scroll.add_css_class("editor-sidebar");

        let on_open: Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>> =
            Rc::new(RefCell::new(None));

        let ft = Self {
            widget: scroll,
            store,
            tree_view: tree_view.clone(),
            root: Rc::new(RefCell::new(None)),
            on_open,
        };

        let on_open_cb = ft.on_open.clone();
        let store_ref = ft.store.clone();
        tree_view.connect_row_activated(move |_, path, _| {
            if let Some(iter) = TreeModelExt::iter(&store_ref, path) {
                let is_dir = col_bool(&store_ref, &iter, COL_IS_DIR);
                if !is_dir {
                    let path_str = col_string(&store_ref, &iter, COL_PATH);
                    if !path_str.is_empty() {
                        if let Some(cb) = on_open_cb.borrow().as_ref() {
                            cb(PathBuf::from(path_str));
                        }
                    }
                }
            }
        });

        let store_lazy = ft.store.clone();
        tree_view.connect_row_expanded(move |_tv, iter, _tree_path| {
            if let Some(child) = TreeModelExt::iter_children(&store_lazy, Some(iter)) {
                let name = col_string(&store_lazy, &child, COL_NAME);
                if name.is_empty() {
                    store_lazy.remove(&child);
                    let dir_path = col_string(&store_lazy, iter, COL_PATH);
                    if !dir_path.is_empty() {
                        populate_dir(&store_lazy, iter, &PathBuf::from(dir_path));
                    }
                }
            }
        });

        ft
    }

    pub fn on_open<F: Fn(PathBuf) + 'static>(&self, cb: F) {
        *self.on_open.borrow_mut() = Some(Box::new(cb));
    }

    pub fn set_root(&self, root: &Path) {
        self.store.clear();
        *self.root.borrow_mut() = Some(root.to_path_buf());

        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root.to_string_lossy().to_string());

        let root_iter = self.store.append(None);
        let root_path_str = root.to_string_lossy().to_string();
        self.store.set(&root_iter, &[
            (COL_NAME, &root_name.as_str()),
            (COL_PATH, &root_path_str.as_str()),
            (COL_IS_DIR, &true),
        ]);

        populate_dir(&self.store, &root_iter, root);

        let tree_path = TreeModelExt::path(&self.store, &root_iter);
        self.tree_view.expand_row(&tree_path, false);
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.root.borrow().clone()
    }
}

fn populate_dir(store: &TreeStore, parent_iter: &TreeIter, dir: &Path) {
    let mut entries: Vec<(bool, String, PathBuf)> = Vec::new();

    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let is_dir = path.is_dir();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            entries.push((is_dir, name, path));
        }
    }

    entries.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });

    for (is_dir, name, path) in entries {
        let iter = store.append(Some(parent_iter));
        let display = if is_dir {
            format!("▸ {}", name)
        } else {
            format!("  {}", name)
        };
        let path_str = path.to_string_lossy().to_string();
        store.set(&iter, &[
            (COL_NAME, &display.as_str()),
            (COL_PATH, &path_str.as_str()),
            (COL_IS_DIR, &is_dir),
        ]);

        if is_dir {
            let placeholder = store.append(Some(&iter));
            store.set(&placeholder, &[
                (COL_NAME, &""),
                (COL_PATH, &""),
                (COL_IS_DIR, &false),
            ]);
        }
    }
}
