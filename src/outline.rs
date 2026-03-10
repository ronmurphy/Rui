//! outline.rs — Widget hierarchy outline panel.
//!
//! Parses the active .ui buffer with roxmltree and displays every
//! `<object>` node as an indented, clickable row. Clicking a row
//! moves the editor cursor to that widget's `<object>` tag so the
//! inspector and canvas selection update automatically.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow,
    SelectionMode,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const DEBOUNCE_MS: u64 = 400;

/// One entry in the flat outline list.
struct OutlineEntry {
    class:       String,
    id:          Option<String>,
    depth:       usize,
    /// Char offset in the buffer: positioned just inside the `<object` tag
    /// so `inspector::find_object_at_offset` picks up this widget.
    char_offset: i32,
}

/// The widget outline panel.
#[derive(Clone)]
pub struct OutlinePanel {
    pub widget:    GtkBox,
    list_box:      ListBox,
    target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
    entries:       Rc<RefCell<Vec<OutlineEntry>>>,
    generation:    Rc<Cell<u64>>,
}

impl OutlinePanel {
    pub fn new() -> Self {
        let header = Label::new(Some("Widget Tree"));
        header.set_halign(gtk4::Align::Start);
        header.set_margin_start(8);
        header.set_margin_top(6);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let list_box = ListBox::new();
        list_box.set_selection_mode(SelectionMode::Single);
        list_box.add_css_class("navigation-sidebar");

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&list_box)
            .build();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.append(&header);
        widget.append(&scroll);

        let entries: Rc<RefCell<Vec<OutlineEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>> =
            Rc::new(RefCell::new(None));
        let generation = Rc::new(Cell::new(0u64));

        // Wire row-activated once — reads from `entries` Rc on every click.
        {
            let entries_ref = entries.clone();
            let buf_ref = target_buffer.clone();
            list_box.connect_row_activated(move |_, row| {
                let idx = row.index() as usize;
                let entries = entries_ref.borrow();
                if let Some(entry) = entries.get(idx) {
                    if let Some(buffer) = buf_ref.borrow().as_ref() {
                        let iter = buffer.iter_at_offset(entry.char_offset);
                        buffer.place_cursor(&iter);
                    }
                }
            });
        }

        OutlinePanel {
            widget,
            list_box,
            target_buffer,
            entries,
            generation,
        }
    }

    /// Switch to a new buffer (tab switch, no changed-signal reconnect).
    pub fn set_buffer(&self, buffer: &sourceview5::Buffer) {
        *self.target_buffer.borrow_mut() = Some(buffer.clone());
    }

    pub fn clear_buffer(&self) {
        *self.target_buffer.borrow_mut() = None;
        self.clear_list();
    }

    /// Connect to a .ui buffer: initial render + debounced refresh on every edit.
    pub fn connect_buffer(&self, buffer: &sourceview5::Buffer) {
        self.set_buffer(buffer);
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false).to_string();
        self.rebuild(&text);

        let outline = self.clone();
        let buf = buffer.clone();
        buffer.connect_changed(move |_| {
            let gen = outline.generation.get().wrapping_add(1);
            outline.generation.set(gen);
            let outline2 = outline.clone();
            let buf2 = buf.clone();
            let expected = outline.generation.clone();
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(DEBOUNCE_MS),
                move || {
                    if expected.get() == gen {
                        let (s, e) = buf2.bounds();
                        let text = buf2.text(&s, &e, false).to_string();
                        outline2.rebuild(&text);
                    }
                },
            );
        });
    }

    fn clear_list(&self) {
        self.entries.borrow_mut().clear();
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
    }

    fn rebuild(&self, xml: &str) {
        self.clear_list();

        let doc = match roxmltree::Document::parse(xml) {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut entries: Vec<OutlineEntry> = Vec::new();
        let root = doc.root_element();

        if root.tag_name().name() == "interface" {
            for node in root
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "object")
            {
                collect_entries(node, 0, xml, &mut entries);
            }
        } else if root.tag_name().name() == "object" {
            collect_entries(root, 0, xml, &mut entries);
        }

        // Build ListBox rows from the flat entry list.
        for entry in &entries {
            let row = ListBoxRow::new();
            let row_box = GtkBox::new(Orientation::Horizontal, 4);
            row_box.set_margin_start(6 + entry.depth as i32 * 16);
            row_box.set_margin_end(6);
            row_box.set_margin_top(2);
            row_box.set_margin_bottom(2);

            let icon = if is_container(&entry.class) { "▸" } else { "•" };
            let icon_lbl = Label::new(Some(icon));
            icon_lbl.set_margin_end(2);

            let name_lbl = Label::new(Some(short_class(&entry.class)));
            name_lbl.set_halign(gtk4::Align::Start);
            name_lbl.set_hexpand(true);

            row_box.append(&icon_lbl);
            row_box.append(&name_lbl);

            if let Some(ref id) = entry.id {
                let id_lbl = Label::new(Some(&format!("#{id}")));
                id_lbl.add_css_class("dim-label");
                id_lbl.set_halign(gtk4::Align::End);
                row_box.append(&id_lbl);
            }

            row.set_child(Some(&row_box));
            self.list_box.append(&row);
        }

        *self.entries.borrow_mut() = entries;
    }
}

/// Recursively walk `<object>` → `<child>` → `<object>` collecting entries.
fn collect_entries(
    node: roxmltree::Node,
    depth: usize,
    xml: &str,
    entries: &mut Vec<OutlineEntry>,
) {
    let class = match node.attribute("class") {
        Some(c) => c.to_string(),
        None => return,
    };
    let id = node.attribute("id").map(|s| s.to_string());

    // Position cursor just inside "<object" so the inspector finds this node.
    let byte_offset = (node.range().start + 7).min(xml.len());
    let char_offset = xml[..byte_offset].chars().count() as i32;

    entries.push(OutlineEntry { class: class.clone(), id, depth, char_offset });

    for child_node in node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "child")
    {
        for obj in child_node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "object")
        {
            collect_entries(obj, depth + 1, xml, entries);
        }
    }
}

fn short_class(class: &str) -> &str {
    class.strip_prefix("Gtk").unwrap_or(class)
}

fn is_container(class: &str) -> bool {
    matches!(
        class,
        "GtkBox" | "GtkGrid" | "GtkFrame" | "GtkScrolledWindow"
            | "GtkPaned" | "GtkNotebook" | "GtkStack" | "GtkOverlay"
            | "GtkCenterBox" | "GtkExpander" | "GtkFlowBox" | "GtkListBox"
            | "GtkHeaderBar" | "GtkActionBar" | "GtkWindow"
            | "GtkApplicationWindow" | "GtkDialog" | "GtkPopover"
    )
}
