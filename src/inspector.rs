//! inspector.rs — Property inspector panel.
//!
//! When the cursor is inside an `<object>` block in a .ui file, the inspector
//! shows the widget's class, id, and all `<property>` elements as editable
//! rows. Editing a property value in the inspector writes it back to the XML
//! buffer immediately.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Entry, Label, Orientation, ScrolledWindow, Separator,
};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Parsed representation of a widget from the XML.
struct WidgetInfo {
    class: String,
    id: Option<String>,
    /// (property_name, value, byte_offset_of_value_start, byte_offset_of_value_end)
    properties: Vec<(String, String, usize, usize)>,
}

/// The property inspector panel.
#[derive(Clone)]
pub struct Inspector {
    pub widget: GtkBox,
    content: GtkBox,
    header_label: Label,
    target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
    /// Currently displayed properties — cleared and rebuilt on cursor move.
    entries: Rc<RefCell<Vec<(String, Entry)>>>,
    /// Guard flag: true while clearing the panel to suppress focus-leave callbacks.
    clearing: Rc<Cell<bool>>,
}

impl Inspector {
    pub fn new() -> Self {
        let header_label = Label::new(Some("Properties"));
        header_label.set_halign(gtk4::Align::Start);
        header_label.set_margin_start(8);
        header_label.set_margin_top(6);
        header_label.set_margin_bottom(2);
        header_label.add_css_class("heading");

        let content = GtkBox::new(Orientation::Vertical, 2);
        content.set_margin_start(4);
        content.set_margin_end(4);

        let scroll = ScrolledWindow::builder()
            .hexpand(false)
            .vexpand(true)
            .child(&content)
            .build();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(220);
        widget.append(&header_label);
        widget.append(&scroll);

        Inspector {
            widget,
            content,
            header_label,
            target_buffer: Rc::new(RefCell::new(None)),
            entries: Rc::new(RefCell::new(Vec::new())),
            clearing: Rc::new(Cell::new(false)),
        }
    }

    /// Set which buffer to inspect (call on tab switch).
    pub fn set_buffer(&self, buffer: &sourceview5::Buffer) {
        *self.target_buffer.borrow_mut() = Some(buffer.clone());
    }

    pub fn clear_buffer(&self) {
        *self.target_buffer.borrow_mut() = None;
        self.clear_panel();
    }

    /// Connect to a buffer's cursor-moved signal to update the inspector
    /// whenever the cursor enters a different `<object>` block.
    pub fn connect_buffer(&self, buffer: &sourceview5::Buffer) {
        self.set_buffer(buffer);

        let inspector = self.clone();
        let buf = buffer.clone();

        // GtkTextBuffer emits "notify::cursor-position" when the insert mark moves
        buffer.connect_cursor_position_notify(move |_| {
            inspector.update_from_cursor(&buf);
        });
    }

    /// Re-read the XML around the cursor and rebuild the property panel.
    fn update_from_cursor(&self, buffer: &sourceview5::Buffer) {
        let (start, end) = buffer.bounds();
        let full_text = buffer.text(&start, &end, false).to_string();
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        let cursor_offset = cursor.offset() as usize;

        // Convert char offset to byte offset (approximate — works for ASCII-heavy XML)
        let byte_offset = full_text
            .char_indices()
            .nth(cursor_offset)
            .map(|(i, _)| i)
            .unwrap_or(full_text.len());

        match find_object_at_offset(&full_text, byte_offset) {
            Some(info) => self.show_widget_info(buffer, &info),
            None => self.clear_panel(),
        }
    }

    fn clear_panel(&self) {
        // Set clearing flag so focus-leave callbacks are suppressed
        self.clearing.set(true);
        // Remove all children from content
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        self.entries.borrow_mut().clear();
        self.header_label.set_text("Properties");
        self.clearing.set(false);
    }

    fn show_widget_info(&self, buffer: &sourceview5::Buffer, info: &WidgetInfo) {
        self.clear_panel();

        let title = match &info.id {
            Some(id) => format!("{} — #{}", info.class, id),
            None => info.class.clone(),
        };
        self.header_label.set_text(&title);

        // Class label
        let class_row = make_label_row("class", &info.class);
        self.content.append(&class_row);

        if let Some(id) = &info.id {
            let id_row = make_label_row("id", id);
            self.content.append(&id_row);
        }

        let sep = Separator::new(Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        self.content.append(&sep);

        // Editable property rows
        let mut entries = self.entries.borrow_mut();
        for (name, value, val_start, val_end) in &info.properties {
            let row = GtkBox::new(Orientation::Vertical, 1);
            row.set_margin_top(2);
            row.set_margin_bottom(2);

            let name_label = Label::new(Some(name));
            name_label.set_halign(gtk4::Align::Start);
            name_label.set_margin_start(4);
            name_label.add_css_class("dim-label");

            let entry = Entry::new();
            entry.set_text(value);
            entry.add_css_class("monospace");

            row.append(&name_label);
            row.append(&entry);
            self.content.append(&row);

            // When the user presses Enter or leaves the entry, write back to XML
            let buf = buffer.clone();
            let vs = *val_start;
            let ve = *val_end;
            let entry_clone = entry.clone();
            entry.connect_activate(move |_| {
                write_property_back(&buf, vs, ve, &entry_clone.text());
            });

            // Also write back when focus leaves the entry
            {
                let buf2 = buffer.clone();
                let vs2 = *val_start;
                let ve2 = *val_end;
                let entry2 = entry.clone();
                let clearing = self.clearing.clone();
                let focus = gtk4::EventControllerFocus::new();
                focus.connect_leave(move |_| {
                    // Don't write back if we're clearing the panel (entries being removed)
                    if !clearing.get() {
                        write_property_back(&buf2, vs2, ve2, &entry2.text());
                    }
                });
                entry.add_controller(focus);
            }

            entries.push((name.clone(), entry));
        }
    }

    pub fn toggle(&self) {
        self.widget.set_visible(!self.widget.is_visible());
    }
}

/// Find the `<object>` element surrounding the given byte offset.
fn find_object_at_offset(xml: &str, offset: usize) -> Option<WidgetInfo> {
    // Snap offset to a valid char boundary
    let offset = snap_to_char_boundary(xml, offset);

    // Walk backwards from offset to find the nearest `<object` opening
    let before = &xml[..offset];
    let obj_start = before.rfind("<object")?;;

    // Find the matching `</object>` — handle nesting
    let after_start = &xml[obj_start..];
    let obj_end = find_closing_object(after_start)?;
    let block = &xml[obj_start..obj_start + obj_end];

    // Make sure the cursor is actually inside this block
    if offset > obj_start + obj_end {
        return None;
    }

    // Extract class
    let class = extract_attr(block, "class")?;

    // Extract id (optional)
    let id = extract_attr(block, "id");

    // Extract properties
    let mut properties = Vec::new();
    let mut search_from = 0;
    while let Some(prop_start) = block[search_from..].find("<property name=\"") {
        let abs_prop = search_from + prop_start;
        let tag_content = &block[abs_prop..];

        // Get property name
        let name_start = "<property name=\"".len();
        let name_end = tag_content[name_start..].find('"')?;
        let prop_name = tag_content[name_start..name_start + name_end].to_string();

        // Find the > that closes the opening tag
        let gt = tag_content.find('>')?;
        let val_start_in_block = abs_prop + gt + 1;

        // Find </property>
        let close = tag_content.find("</property>")?;
        let val_end_in_block = abs_prop + close;

        let value = block[val_start_in_block..val_end_in_block].to_string();

        // Convert block-relative offsets to absolute XML offsets
        let abs_val_start = obj_start + val_start_in_block;
        let abs_val_end = obj_start + val_end_in_block;

        properties.push((prop_name, value, abs_val_start, abs_val_end));

        search_from = abs_prop + close + "</property>".len();
    }

    Some(WidgetInfo {
        class,
        id,
        properties,
    })
}

/// Find the byte offset of the closing `</object>` for the first `<object` in the string,
/// handling nested `<object>` elements.
fn find_closing_object(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut pos = 0;
    while pos < s.len() {
        if s[pos..].starts_with("<object") {
            depth += 1;
            pos += 7;
        } else if s[pos..].starts_with("</object>") {
            depth -= 1;
            if depth == 0 {
                return Some(pos + "</object>".len());
            }
            pos += 9;
        } else {
            // Advance by one UTF-8 character, not one byte
            let ch_len = s[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            pos += ch_len;
        }
    }
    None
}

/// Extract an XML attribute value: `attr="value"` → `value`
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// Write an edited property value back into the buffer at the original byte offsets.
fn write_property_back(buffer: &sourceview5::Buffer, val_start: usize, val_end: usize, new_value: &str) {
    let (buf_start, buf_end) = buffer.bounds();
    let full_text = buffer.text(&buf_start, &buf_end, false).to_string();

    // Snap byte offsets to valid char boundaries before counting chars
    let safe_start = snap_to_char_boundary(&full_text, val_start);
    let safe_end = snap_to_char_boundary(&full_text, val_end);

    // Convert byte offsets to char offsets
    let char_start = full_text[..safe_start].chars().count();
    let char_end = full_text[..safe_end].chars().count();

    let mut start_iter = buffer.iter_at_offset(char_start as i32);
    let mut end_iter = buffer.iter_at_offset(char_end as i32);

    buffer.begin_user_action();
    buffer.delete(&mut start_iter, &mut end_iter);
    buffer.insert(&mut start_iter, new_value);
    buffer.end_user_action();
}

/// Snap a byte offset to the nearest valid UTF-8 char boundary (rounding down).
fn snap_to_char_boundary(s: &str, offset: usize) -> usize {
    let offset = offset.min(s.len());
    // Walk backwards to find a valid char boundary
    let mut pos = offset;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn make_label_row(label: &str, value: &str) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 4);
    row.set_margin_start(4);
    row.set_margin_top(2);
    let l = Label::new(Some(label));
    l.add_css_class("dim-label");
    l.set_width_chars(8);
    l.set_halign(gtk4::Align::Start);
    let v = Label::new(Some(value));
    v.set_halign(gtk4::Align::Start);
    v.set_selectable(true);
    row.append(&l);
    row.append(&v);
    row
}
