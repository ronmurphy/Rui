//! outline.rs — Widget hierarchy outline panel with drag-and-drop reordering.
//!
//! Parses the active .ui buffer with roxmltree and displays every `<object>`
//! node as an indented, clickable row.  Rows can be dragged onto other rows
//! to move the corresponding `<child>` block in the XML source, which
//! immediately re-renders the canvas.
//!
//! Drop semantics:
//!   • Drop onto a **container** row  → widget moves *inside* that container
//!     as its last child.
//!   • Drop onto a **leaf** row       → widget moves *before* the target
//!     (top half of row) or *after* it (bottom half).

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DragSource, DropTarget, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SelectionMode,
};
use gtk4::gdk::{ContentProvider, DragAction};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::ops::Range;

/// Flat widget description passed to the VB6-style selector combo in the Toolbox.
pub struct SelectorItem {
    pub label:       String,
    pub byte_offset: usize,
    /// Byte range of the wrapping `<child>` block, if any (None for root objects).
    pub child_range: Option<(usize, usize)>,
}

const DEBOUNCE_MS: u64 = 400;

/// One entry in the flat outline list.
struct OutlineEntry {
    class:             String,
    id:                Option<String>,
    depth:             usize,
    /// Char offset for cursor jump (existing behaviour).
    char_offset:       i32,
    /// Raw byte offset (node.range().start + 7) for canvas tree-highlight.
    byte_offset:       usize,
    /// Byte range of the `<child>…</child>` block wrapping this object.
    /// `None` for objects that are direct children of `<interface>`.
    child_byte_range:  Option<Range<usize>>,
    /// Byte range of the full `<object>…</object>` element.
    object_byte_range: Range<usize>,
}

/// The widget outline panel.
#[derive(Clone)]
pub struct OutlinePanel {
    pub widget:    GtkBox,
    list_box:      ListBox,
    target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
    entries:       Rc<RefCell<Vec<OutlineEntry>>>,
    generation:    Rc<Cell<u64>>,
    /// Called with the byte_offset of the selected entry so the canvas can highlight it.
    on_select_cb:  Rc<RefCell<Option<Box<dyn Fn(usize)>>>>,
    /// Called after every rebuild with the flat widget list for the selector combo.
    on_rebuild_cb: Rc<RefCell<Option<Box<dyn Fn(Vec<SelectorItem>)>>>>,
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
        list_box.set_margin_start(4);
        list_box.set_margin_end(4);
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

        let on_select_cb:  Rc<RefCell<Option<Box<dyn Fn(usize)>>>>           = Rc::new(RefCell::new(None));
        let on_rebuild_cb: Rc<RefCell<Option<Box<dyn Fn(Vec<SelectorItem>)>>>> = Rc::new(RefCell::new(None));

        // Cursor-jump on row click; also fires the canvas highlight callback.
        {
            let entries_ref = entries.clone();
            let buf_ref = target_buffer.clone();
            let cb_ref = on_select_cb.clone();
            list_box.connect_row_activated(move |_, row| {
                let idx = row.index() as usize;
                let entries = entries_ref.borrow();
                if let Some(entry) = entries.get(idx) {
                    if let Some(buffer) = buf_ref.borrow().as_ref() {
                        let iter = buffer.iter_at_offset(entry.char_offset);
                        buffer.place_cursor(&iter);
                    }
                    if let Some(cb) = cb_ref.borrow().as_ref() {
                        cb(entry.byte_offset);
                    }
                }
            });
        }

        OutlinePanel { widget, list_box, target_buffer, entries, generation, on_select_cb, on_rebuild_cb }
    }

    /// Register a callback fired when a tree row is clicked.
    /// The callback receives the byte_offset of the selected `<object>` node
    /// (suitable for passing to `Canvas::select_from_tree`).
    pub fn on_row_select<F: Fn(usize) + 'static>(&self, cb: F) {
        *self.on_select_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback fired after every outline rebuild.
    /// Receives a flat `Vec<SelectorItem>` for populating the toolbox selector combo.
    pub fn on_rebuild<F: Fn(Vec<SelectorItem>) + 'static>(&self, cb: F) {
        *self.on_rebuild_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Delete the `<child>` block spanning `[start, end)` from the active buffer.
    /// Used by the toolbox selector trash button.
    pub fn delete_child_bytes(&self, start: usize, end: usize) {
        let buf = self.target_buffer.borrow();
        if let Some(buffer) = buf.as_ref() {
            let (s, e) = buffer.bounds();
            let xml = buffer.text(&s, &e, false).to_string();
            if start > xml.len() || end > xml.len() { return; }
            let remove_start = xml[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let remove_end   = if end < xml.len() && xml.as_bytes()[end] == b'\n' {
                end + 1
            } else {
                end
            };
            let char_start = xml[..remove_start].chars().count();
            let char_end   = xml[..remove_end].chars().count();
            let mut si = buffer.iter_at_offset(char_start as i32);
            let mut ei = buffer.iter_at_offset(char_end   as i32);
            buffer.begin_user_action();
            buffer.delete(&mut si, &mut ei);
            buffer.end_user_action();
        }
    }

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

        let mut new_entries: Vec<OutlineEntry> = Vec::new();
        let root = doc.root_element();

        if root.tag_name().name() == "interface" {
            for node in root
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "object")
            {
                collect_entries(node, None, 0, xml, &mut new_entries);
            }
        } else if root.tag_name().name() == "object" {
            collect_entries(root, None, 0, xml, &mut new_entries);
        }

        // ── Build ListBox rows ───────────────────────────────────────────────
        for (idx, entry) in new_entries.iter().enumerate() {
            let row     = ListBoxRow::new();
            let row_box = GtkBox::new(Orientation::Horizontal, 4);
            row_box.set_margin_start(6 + entry.depth as i32 * 16);
            row_box.set_margin_end(6);
            row_box.set_margin_top(2);
            row_box.set_margin_bottom(2);

            // Nerd Font icon: folder for containers, file for leaves.
            let is_cont  = is_container(&entry.class);
            let icon_chr = if is_cont { "\u{F07B}" } else { "\u{F016}" };
            let icon_lbl = Label::new(Some(icon_chr));
            icon_lbl.add_css_class("nf");
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

            // Delete button — only for widgets wrapped in a <child> block.
            if let Some(ref cr) = entry.child_byte_range {
                let del_lbl = Label::new(Some("\u{F1F8}")); // nf-fa-trash
                del_lbl.add_css_class("nf");
                let del_btn = Button::new();
                del_btn.set_child(Some(&del_lbl));
                del_btn.add_css_class("flat");
                del_btn.set_valign(gtk4::Align::Center);
                del_btn.set_tooltip_text(Some("Delete widget"));

                let buf_rc   = self.target_buffer.clone();
                let cr_start = cr.start;
                let cr_end   = cr.end;
                del_btn.connect_clicked(move |_| {
                    let buf = buf_rc.borrow();
                    if let Some(buffer) = buf.as_ref() {
                        let (s, e)   = buffer.bounds();
                        let xml      = buffer.text(&s, &e, false).to_string();
                        let remove_start = xml[..cr_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                        let remove_end   = if cr_end < xml.len() && xml.as_bytes()[cr_end] == b'\n' {
                            cr_end + 1
                        } else {
                            cr_end
                        };
                        let char_start = xml[..remove_start].chars().count();
                        let char_end   = xml[..remove_end].chars().count();
                        let mut si = buffer.iter_at_offset(char_start as i32);
                        let mut ei = buffer.iter_at_offset(char_end   as i32);
                        buffer.begin_user_action();
                        buffer.delete(&mut si, &mut ei);
                        buffer.end_user_action();
                    }
                });
                row_box.append(&del_btn);
            }

            row.set_child(Some(&row_box));

            // ── DragSource (only rows backed by a <child> wrapper) ───────────
            if let Some(ref cr) = entry.child_byte_range {
                // Payload: "start:end" of the <child> block — mirrors canvas.rs
                let payload = format!("{}:{}", cr.start, cr.end);
                let drag_src = DragSource::new();
                drag_src.set_actions(DragAction::MOVE);
                let content = ContentProvider::for_value(&payload.to_value());
                drag_src.set_content(Some(&content));

                {
                    let row2 = row.clone();
                    drag_src.connect_drag_begin(move |_, _| {
                        row2.add_css_class("drag-active");
                    });
                }
                {
                    let row2 = row.clone();
                    drag_src.connect_drag_end(move |_, _, _| {
                        row2.remove_css_class("drag-active");
                    });
                }
                row_box.add_controller(drag_src);
            }

            // ── DropTarget (every row is a potential drop site) ─────────────
            {
                let entries_rc       = self.entries.clone();
                let buf_rc           = self.target_buffer.clone();
                let dst_idx          = idx;
                let dst_is_container = is_cont;

                let drop_tgt = DropTarget::new(String::static_type(), DragAction::MOVE);

                {
                    let row2 = row.clone();
                    drop_tgt.connect_enter(move |_, _, _| {
                        row2.add_css_class("drop-hover");
                        DragAction::MOVE
                    });
                }
                {
                    let row2 = row.clone();
                    drop_tgt.connect_leave(move |_| {
                        row2.remove_css_class("drop-hover");
                    });
                }

                drop_tgt.connect_drop(move |_, val, _x, y| {
                    let result: Option<bool> = (|| {
                        let payload    = val.get::<String>().ok()?;
                        let src_child  = parse_child_range(&payload)?;

                        let entries    = entries_rc.borrow();
                        let src_entry  = entries.iter().find(|e| {
                            e.child_byte_range.as_ref() == Some(&src_child)
                        })?;
                        let src_idx    = entries.iter().position(|e| {
                            e.child_byte_range.as_ref() == Some(&src_child)
                        })?;
                        if src_idx == dst_idx { return Some(false); }

                        let dst_entry      = entries.get(dst_idx)?;
                        let src_depth      = src_entry.depth;
                        let dst_obj_range  = dst_entry.object_byte_range.clone();
                        let dst_child_range = dst_entry.child_byte_range.clone();

                        // Prevent dropping into own subtree.
                        if dst_is_container && dst_idx > src_idx {
                            let between    = &entries[src_idx + 1..dst_idx];
                            let is_desc    = between.iter().all(|e| e.depth > src_depth)
                                && dst_entry.depth > src_depth;
                            if is_desc { return Some(false); }
                        }
                        drop(entries);

                        // Read current XML.
                        let buf      = buf_rc.borrow();
                        let buffer   = buf.as_ref()?;
                        let (s, e)   = buffer.bounds();
                        let xml_text = buffer.text(&s, &e, false).to_string();
                        drop(buf);

                        // Compute new XML.
                        // y < 14.0 → insert before target; ≥ 14.0 → after.
                        let insert_after = !dst_is_container && y >= 14.0;
                        let new_xml = if dst_is_container {
                            move_child_into(&xml_text, &src_child, &dst_obj_range)?
                        } else {
                            let dcr = dst_child_range.as_ref()?;
                            move_child_adjacent(&xml_text, &src_child, dcr, insert_after)?
                        };

                        // Apply atomically as one undoable user action.
                        let buf2   = buf_rc.borrow();
                        let buffer = buf2.as_ref()?;
                        buffer.begin_user_action();
                        let (mut s2, mut e2) = (buffer.start_iter(), buffer.end_iter());
                        buffer.delete(&mut s2, &mut e2);
                        buffer.insert(&mut buffer.end_iter(), &new_xml);
                        buffer.end_user_action();
                        Some(true)
                    })();
                    result.unwrap_or(false)
                });

                row_box.add_controller(drop_tgt);
            }

            self.list_box.append(&row);
        }

        // Build flat selector items and fire the rebuild callback.
        let selector_items: Vec<SelectorItem> = new_entries.iter().map(|e| {
            let label = if let Some(ref id) = e.id {
                format!("{} #{id}", short_class(&e.class))
            } else {
                short_class(&e.class).to_string()
            };
            SelectorItem {
                label,
                byte_offset: e.byte_offset,
                child_range: e.child_byte_range.as_ref().map(|r| (r.start, r.end)),
            }
        }).collect();
        if let Some(cb) = self.on_rebuild_cb.borrow().as_ref() {
            cb(selector_items);
        }

        *self.entries.borrow_mut() = new_entries;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// XML helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Move `src_child` block *inside* `dst_obj` (as its last child, before `</object>`).
fn move_child_into(
    xml:     &str,
    src:     &Range<usize>,
    dst_obj: &Range<usize>,
) -> Option<String> {
    // Reject if src is already inside dst.
    if src.start >= dst_obj.start && src.end <= dst_obj.end {
        return None;
    }

    let src_block = xml[src.clone()].to_string();

    // Find the closing </object> of the destination container.
    // rfind gives us the LAST one in the slice, which closes the outer object.
    let dst_xml   = &xml[dst_obj.clone()];
    let close_pos = dst_xml.rfind("</object>")?;
    let insert_at = dst_obj.start + close_pos;

    splice(xml, src, insert_at, &src_block)
}

/// Move `src_child` block immediately before or after `dst_child` block.
fn move_child_adjacent(
    xml:          &str,
    src:          &Range<usize>,
    dst:          &Range<usize>,
    insert_after: bool,
) -> Option<String> {
    if src == dst { return None; }
    let insert_at = if insert_after { dst.end } else { dst.start };
    let src_block = xml[src.clone()].to_string();
    splice(xml, src, insert_at, &src_block)
}

/// Remove `src` from `xml`, then insert `block` at the adjusted `insert_at`.
fn splice(xml: &str, src: &Range<usize>, insert_at: usize, block: &str) -> Option<String> {
    if src.end <= insert_at {
        // src is before the insertion point — remove src first, then adjust.
        let after_remove = format!("{}{}", &xml[..src.start], &xml[src.end..]);
        let adj          = insert_at - block.len();
        Some(format!("{}{}{}", &after_remove[..adj], block, &after_remove[adj..]))
    } else if insert_at <= src.start {
        // insertion point is before src — insert first, then remove (now shifted).
        let after_insert  = format!("{}{}{}", &xml[..insert_at], block, &xml[insert_at..]);
        let new_src_start = src.start + block.len();
        let new_src_end   = src.end   + block.len();
        Some(format!("{}{}", &after_insert[..new_src_start], &after_insert[new_src_end..]))
    } else {
        None // overlapping ranges — shouldn't happen
    }
}

/// Parse a drag payload of the form `"start:end"` into a byte range.
fn parse_child_range(payload: &str) -> Option<Range<usize>> {
    let (a, b) = payload.split_once(':')?;
    Some(a.trim().parse::<usize>().ok()?..b.trim().parse::<usize>().ok()?)
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry collection
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively walk `<object>` → `<child>` → `<object>` collecting entries.
fn collect_entries(
    node:             roxmltree::Node,
    parent_child_range: Option<Range<usize>>,
    depth:            usize,
    xml:              &str,
    entries:          &mut Vec<OutlineEntry>,
) {
    let class = match node.attribute("class") {
        Some(c) => c.to_string(),
        None    => return,
    };
    let id = node.attribute("id").map(|s| s.to_string());

    let byte_offset  = (node.range().start + 7).min(xml.len());
    let char_offset  = xml[..byte_offset].chars().count() as i32;
    let object_byte_range = node.range();

    entries.push(OutlineEntry {
        class: class.clone(),
        id,
        depth,
        char_offset,
        byte_offset,
        child_byte_range: parent_child_range,
        object_byte_range,
    });

    for child_node in node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "child")
    {
        let child_range = child_node.range();
        for obj in child_node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "object")
        {
            collect_entries(obj, Some(child_range.clone()), depth + 1, xml, entries);
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
