//! inspector.rs — Property inspector panel.
//!
//! When the cursor is inside an `<object>` block in a .ui file, the inspector
//! shows the widget's class, id, and all `<property>` elements as editable
//! rows. Editing a property value in the inspector writes it back to the XML
//! buffer immediately.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DropDown, Entry, Grid, Label, Orientation,
    ScrolledWindow, Separator, StringList,
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
    /// Guard flag: true while writing a property back to suppress re-entry.
    writing: Rc<Cell<bool>>,
    // ── Widget selector (moved here from Toolbox) ────────────────────
    selector_dd:           DropDown,
    selector_offsets:      Rc<RefCell<Vec<usize>>>,
    selector_child_ranges: Rc<RefCell<Vec<Option<(usize, usize)>>>>,
    is_populating:         Rc<Cell<bool>>,
    on_select_cb:          Rc<RefCell<Option<Box<dyn Fn(usize)>>>>,
    on_delete_cb:          Rc<RefCell<Option<Box<dyn Fn(usize, usize)>>>>,
}

impl Inspector {
    pub fn new() -> Self {
        let header_label = Label::new(Some("Properties"));
        header_label.set_halign(gtk4::Align::Start);
        header_label.set_margin_start(8);
        header_label.set_margin_top(6);
        header_label.set_margin_bottom(2);
        header_label.add_css_class("heading");

        // ── Widget selector row ──────────────────────────────────────
        let selector_offsets:      Rc<RefCell<Vec<usize>>>                  = Rc::new(RefCell::new(Vec::new()));
        let selector_child_ranges: Rc<RefCell<Vec<Option<(usize, usize)>>>> = Rc::new(RefCell::new(Vec::new()));
        let is_populating  = Rc::new(Cell::new(false));
        let on_select_cb:  Rc<RefCell<Option<Box<dyn Fn(usize)>>>>          = Rc::new(RefCell::new(None));
        let on_delete_cb:  Rc<RefCell<Option<Box<dyn Fn(usize, usize)>>>>   = Rc::new(RefCell::new(None));

        let selector_dd = DropDown::builder()
            .model(&StringList::new(&[]))
            .hexpand(true)
            .tooltip_text("Select widget")
            .build();

        {
            let is_pop  = is_populating.clone();
            let offsets = selector_offsets.clone();
            let cb_ref  = on_select_cb.clone();
            selector_dd.connect_selected_notify(move |dd| {
                if is_pop.get() { return; }
                let idx = dd.selected();
                if idx == gtk4::INVALID_LIST_POSITION { return; }
                if let Some(&byte_offset) = offsets.borrow().get(idx as usize) {
                    if let Some(cb) = cb_ref.borrow().as_ref() {
                        cb(byte_offset);
                    }
                }
            });
        }

        let trash_lbl = Label::new(Some("\u{F1F8}"));
        trash_lbl.add_css_class("nf");
        let trash_btn = Button::new();
        trash_btn.set_child(Some(&trash_lbl));
        trash_btn.add_css_class("flat");
        trash_btn.set_valign(gtk4::Align::Center);
        trash_btn.set_tooltip_text(Some("Delete selected widget"));

        {
            let dd_ref = selector_dd.clone();
            let ranges = selector_child_ranges.clone();
            let cb_ref = on_delete_cb.clone();
            trash_btn.connect_clicked(move |_| {
                let idx = dd_ref.selected();
                if idx == gtk4::INVALID_LIST_POSITION { return; }
                if let Some(Some((start, end))) = ranges.borrow().get(idx as usize) {
                    if let Some(cb) = cb_ref.borrow().as_ref() {
                        cb(*start, *end);
                    }
                }
            });
        }

        let sel_row = GtkBox::new(Orientation::Horizontal, 4);
        sel_row.set_margin_start(6);
        sel_row.set_margin_end(6);
        sel_row.set_margin_top(4);
        sel_row.set_margin_bottom(4);
        sel_row.append(&selector_dd);
        sel_row.append(&trash_btn);
        // ─────────────────────────────────────────────────────────────

        let content = GtkBox::new(Orientation::Vertical, 2);
        content.set_margin_start(4);
        content.set_margin_end(4);

        let scroll = ScrolledWindow::builder()
            .hexpand(false)
            .vexpand(true)
            .child(&content)
            .build();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.append(&header_label);
        widget.append(&sel_row);
        widget.append(&scroll);

        Inspector {
            widget,
            content,
            header_label,
            target_buffer: Rc::new(RefCell::new(None)),
            entries: Rc::new(RefCell::new(Vec::new())),
            clearing: Rc::new(Cell::new(false)),
            writing: Rc::new(Cell::new(false)),
            selector_dd,
            selector_offsets,
            selector_child_ranges,
            is_populating,
            on_select_cb,
            on_delete_cb,
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
            // Skip if we're in the middle of writing a property back
            if inspector.writing.get() {
                return;
            }
            inspector.update_from_cursor(&buf);
        });
    }

    /// Show properties for the widget at the given byte offset.
    /// Also moves the buffer cursor there so subsequent property edits target the right widget.
    pub fn update_from_offset(&self, byte_offset: usize) {
        let buf = match self.target_buffer.borrow().as_ref().cloned() {
            Some(b) => b,
            None => return,
        };
        let (start, end) = buf.bounds();
        let full_text = buf.text(&start, &end, false).to_string();
        let safe = byte_offset.min(full_text.len());

        if let Some(info) = find_object_at_offset(&full_text, safe) {
            // Move cursor into the object block so property writes land in the right place.
            // Suppress the cursor-notify handler to avoid a redundant re-render.
            self.writing.set(true);
            let char_offset = full_text[..safe].chars().count();
            let iter = buf.iter_at_offset(char_offset as i32);
            buf.place_cursor(&iter);
            self.writing.set(false);

            self.show_widget_info(&buf, &info);
        }
    }

    /// Re-read the XML around the cursor and rebuild the property panel.
    fn update_from_cursor(&self, buffer: &sourceview5::Buffer) {
        let (start, end) = buffer.bounds();
        let full_text = buffer.text(&start, &end, false).to_string();
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        let cursor_offset = cursor.offset() as usize;

        // Convert char offset to byte offset
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

        let class_row = make_label_row("class", &info.class);
        self.content.append(&class_row);
        if let Some(id) = &info.id {
            self.content.append(&make_label_row("id", id));
        }

        // ── Alignment / expand section ────────────────────────────────
        let sep1 = Separator::new(Orientation::Horizontal);
        sep1.set_margin_top(4);
        sep1.set_margin_bottom(2);
        self.content.append(&sep1);

        let align_heading = Label::new(Some("Alignment & Expand"));
        align_heading.set_halign(gtk4::Align::Start);
        align_heading.set_margin_start(4);
        align_heading.set_margin_bottom(2);
        align_heading.add_css_class("dim-label");
        self.content.append(&align_heading);

        // Read current values (default: fill / false)
        let get_prop = |name: &str| -> String {
            info.properties.iter()
                .find(|(n, ..)| n == name)
                .map(|(_, v, ..)| v.trim().to_string())
                .unwrap_or_default()
        };
        let cur_halign  = get_prop("halign");
        let cur_valign  = get_prop("valign");
        let cur_hexpand = get_prop("hexpand");
        let cur_vexpand = get_prop("vexpand");

        const ALIGN_OPTS: &[&str] = &["fill", "start", "center", "end", "baseline"];
        let align_index = |v: &str| -> u32 {
            ALIGN_OPTS.iter().position(|&s| s == v).unwrap_or(0) as u32
        };

        // Grid: col 0 = label, col 1 = dropdown, col 2 = expand checkbox
        let grid = Grid::new();
        grid.set_column_spacing(4);
        grid.set_row_spacing(4);
        grid.set_margin_start(4);
        grid.set_margin_end(4);
        grid.set_margin_bottom(4);

        // H-align row
        let lbl_h = Label::new(Some("H-align"));
        lbl_h.set_halign(gtk4::Align::Start);
        lbl_h.add_css_class("dim-label");

        let halign_dd = DropDown::new(
            Some(StringList::new(ALIGN_OPTS)),
            gtk4::Expression::NONE,
        );
        halign_dd.set_selected(align_index(cur_halign.trim()));
        halign_dd.set_hexpand(true);
        {
            let buf = buffer.clone();
            let writing = self.writing.clone();
            halign_dd.connect_selected_notify(move |dd| {
                let val = ALIGN_OPTS[dd.selected() as usize];
                add_or_write_property(&buf, "halign", val, &writing);
            });
        }

        let hexpand_cb = CheckButton::with_label("hexpand");
        hexpand_cb.set_active(cur_hexpand.trim() == "true");
        {
            let buf = buffer.clone();
            let writing = self.writing.clone();
            hexpand_cb.connect_toggled(move |cb| {
                let val = if cb.is_active() { "true" } else { "false" };
                add_or_write_property(&buf, "hexpand", val, &writing);
            });
        }

        grid.attach(&lbl_h,     0, 0, 1, 1);
        grid.attach(&halign_dd, 1, 0, 1, 1);
        grid.attach(&hexpand_cb,2, 0, 1, 1);

        // V-align row
        let lbl_v = Label::new(Some("V-align"));
        lbl_v.set_halign(gtk4::Align::Start);
        lbl_v.add_css_class("dim-label");

        let valign_dd = DropDown::new(
            Some(StringList::new(ALIGN_OPTS)),
            gtk4::Expression::NONE,
        );
        valign_dd.set_selected(align_index(cur_valign.trim()));
        valign_dd.set_hexpand(true);
        {
            let buf = buffer.clone();
            let writing = self.writing.clone();
            valign_dd.connect_selected_notify(move |dd| {
                let val = ALIGN_OPTS[dd.selected() as usize];
                add_or_write_property(&buf, "valign", val, &writing);
            });
        }

        let vexpand_cb = CheckButton::with_label("vexpand");
        vexpand_cb.set_active(cur_vexpand.trim() == "true");
        {
            let buf = buffer.clone();
            let writing = self.writing.clone();
            vexpand_cb.connect_toggled(move |cb| {
                let val = if cb.is_active() { "true" } else { "false" };
                add_or_write_property(&buf, "vexpand", val, &writing);
            });
        }

        grid.attach(&lbl_v,     0, 1, 1, 1);
        grid.attach(&valign_dd, 1, 1, 1, 1);
        grid.attach(&vexpand_cb,2, 1, 1, 1);

        self.content.append(&grid);

        // ── Editable property rows — 2-column layout ─────────────────
        let sep2 = Separator::new(Orientation::Horizontal);
        sep2.set_margin_top(2);
        sep2.set_margin_bottom(4);
        self.content.append(&sep2);

        // Collect the properties we'll display (skip the 4 shown above)
        let displayable: Vec<_> = info.properties.iter()
            .filter(|(name, ..)| !matches!(name.as_str(), "halign" | "valign" | "hexpand" | "vexpand"))
            .collect();

        // Two equal-width columns in a horizontal box
        let columns = GtkBox::new(Orientation::Horizontal, 4);
        columns.set_margin_start(2);
        columns.set_margin_end(2);

        let col_left  = GtkBox::new(Orientation::Vertical, 2);
        let col_right = GtkBox::new(Orientation::Vertical, 2);
        col_left.set_hexpand(true);
        col_right.set_hexpand(true);
        columns.append(&col_left);
        columns.append(&col_right);
        self.content.append(&columns);

        let mut entries = self.entries.borrow_mut();
        for (i, (name, value, _val_start, _val_end)) in displayable.iter().enumerate() {
            let col = if i % 2 == 0 { &col_left } else { &col_right };

            let row = GtkBox::new(Orientation::Vertical, 1);
            row.set_margin_top(2);
            row.set_margin_bottom(2);

            let name_label = Label::new(Some(name));
            name_label.set_halign(gtk4::Align::Start);
            name_label.set_margin_start(2);
            name_label.add_css_class("dim-label");

            row.append(&name_label);

            let trimmed = value.trim();
            if let Some(opts) = enum_options_for(name, trimmed) {
                // Known enum property — use a DropDown
                let selected = opts.iter().position(|&o| o == trimmed).unwrap_or(0) as u32;
                let dd = DropDown::new(
                    Some(StringList::new(opts)),
                    gtk4::Expression::NONE,
                );
                dd.set_selected(selected);
                {
                    let buf = buffer.clone();
                    let prop_name = name.to_string();
                    let writing = self.writing.clone();
                    let opts_static = opts;
                    dd.connect_selected_notify(move |dd| {
                        let idx = dd.selected() as usize;
                        let val = opts_static.get(idx).copied().unwrap_or("");
                        write_property_by_name(&buf, &prop_name, val, &writing);
                    });
                }
                row.append(&dd);
            } else {
                let entry = Entry::new();
                entry.set_text(value);
                entry.add_css_class("monospace");

                row.append(&entry);

                {
                    let buf = buffer.clone();
                    let prop_name = name.to_string();
                    let entry_clone = entry.clone();
                    let writing = self.writing.clone();
                    entry.connect_activate(move |_| {
                        write_property_by_name(&buf, &prop_name, &entry_clone.text(), &writing);
                    });
                }
                {
                    let buf2 = buffer.clone();
                    let prop_name2 = name.to_string();
                    let entry2 = entry.clone();
                    let clearing = self.clearing.clone();
                    let writing = self.writing.clone();
                    let focus = gtk4::EventControllerFocus::new();
                    focus.connect_leave(move |_| {
                        if !clearing.get() && !writing.get() {
                            write_property_by_name(&buf2, &prop_name2, &entry2.text(), &writing);
                        }
                    });
                    entry.add_controller(focus);
                }

                entries.push((name.to_string(), entry));
            }

            col.append(&row);
        }

        // ── Add New Property ─────────────────────────────────────────
        let sep3 = Separator::new(Orientation::Horizontal);
        sep3.set_margin_top(6);
        sep3.set_margin_bottom(6);
        self.content.append(&sep3);

        let add_lbl = Label::new(Some("Add Property"));
        add_lbl.set_halign(gtk4::Align::Start);
        add_lbl.set_margin_start(4);
        add_lbl.add_css_class("dim-label");
        self.content.append(&add_lbl);

        let add_box = GtkBox::new(Orientation::Horizontal, 4);
        add_box.set_margin_start(4);
        add_box.set_margin_bottom(8);

        let name_entry = Entry::new();
        name_entry.set_placeholder_text(Some("name"));
        name_entry.set_hexpand(true);

        let val_entry = Entry::new();
        val_entry.set_placeholder_text(Some("value"));
        val_entry.set_hexpand(true);

        let add_btn = gtk4::Button::with_label("+");
        add_btn.add_css_class("suggested-action");

        add_box.append(&name_entry);
        add_box.append(&val_entry);
        add_box.append(&add_btn);
        self.content.append(&add_box);

        {
            let buf = buffer.clone();
            let writing = self.writing.clone();
            let ne = name_entry.clone();
            let ve = val_entry.clone();
            add_btn.connect_clicked(move |_| {
                let p_name = ne.text().to_string();
                let p_val = ve.text().to_string();
                if !p_name.is_empty() {
                    add_or_write_property(&buf, &p_name, &p_val, &writing);
                }
            });
        }
    }

    /// Repopulate the widget selector combo from a fresh outline rebuild.
    pub fn populate_selector(&self, items: Vec<crate::outline::SelectorItem>) {
        self.is_populating.set(true);
        let strings: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        let str_refs: Vec<&str>  = strings.iter().map(String::as_str).collect();
        self.selector_dd.set_model(Some(&StringList::new(&str_refs)));
        *self.selector_offsets.borrow_mut()      = items.iter().map(|i| i.byte_offset).collect();
        *self.selector_child_ranges.borrow_mut() = items.iter().map(|i| i.child_range).collect();
        self.is_populating.set(false);
    }

    /// Register a callback fired when a widget is chosen from the selector combo.
    pub fn on_select_widget<F: Fn(usize) + 'static>(&self, cb: F) {
        *self.on_select_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback fired when the trash button is clicked.
    pub fn on_delete_widget<F: Fn(usize, usize) + 'static>(&self, cb: F) {
        *self.on_delete_cb.borrow_mut() = Some(Box::new(cb));
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
    let obj_start = before.rfind("<object")?;

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

    // Extract properties — only direct properties (not nested child objects)
    let mut properties = Vec::new();
    let mut search_from = 0;
    while let Some(prop_start) = block[search_from..].find("<property name=\"") {
        let abs_prop = search_from + prop_start;
        let tag_content = &block[abs_prop..];

        // Get property name
        let name_start = "<property name=\"".len();
        let name_end = match tag_content[name_start..].find('"') {
            Some(e) => e,
            None => break,
        };
        let prop_name = tag_content[name_start..name_start + name_end].to_string();

        // Find the > that closes the opening tag
        let gt = match tag_content.find('>') {
            Some(g) => g,
            None => break,
        };

        // Skip properties that contain inline objects (e.g. <property name="adjustment"><object...>)
        let val_start_in_block = abs_prop + gt + 1;
        let close = match tag_content.find("</property>") {
            Some(c) => c,
            None => break,
        };
        let val_end_in_block = abs_prop + close;

        let value_text = &block[val_start_in_block..val_end_in_block];

        // Skip if the value contains an <object> (inline child, not editable as text)
        if !value_text.contains("<object") {
            let abs_val_start = obj_start + val_start_in_block;
            let abs_val_end = obj_start + val_end_in_block;
            properties.push((prop_name, value_text.to_string(), abs_val_start, abs_val_end));
        }

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

/// Add a new property or update an existing one, using the cursor position
/// to find the enclosing `<object>` block.
/// If the property already exists, its value is replaced in-place.
/// If it doesn't exist, a new `<property>` line is inserted before `</object>`.
fn add_or_write_property(
    buffer: &sourceview5::Buffer,
    prop_name: &str,
    new_value: &str,
    writing: &Rc<Cell<bool>>,
) {
    if writing.get() { return; }
    writing.set(true);

    let (buf_start, buf_end) = buffer.bounds();
    let full_text = buffer.text(&buf_start, &buf_end, false).to_string();

    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let cursor_char = cursor.offset() as usize;
    let cursor_byte = full_text.char_indices().nth(cursor_char)
        .map(|(i, _)| i).unwrap_or(full_text.len());
    let safe_cursor = snap_to_char_boundary(&full_text, cursor_byte);

    let obj_start = match full_text[..safe_cursor].rfind("<object") {
        Some(s) => s,
        None => { writing.set(false); return; }
    };
    let obj_rest = &full_text[obj_start..];
    let obj_end = match find_closing_object(obj_rest) {
        Some(e) => e,
        None => { writing.set(false); return; }
    };
    let block = &full_text[obj_start..obj_start + obj_end];

    let search_pattern = format!("<property name=\"{}\">", prop_name);
    if let Some(prop_offset) = block.find(&search_pattern) {
        // Property exists — update value in place
        let val_start = obj_start + prop_offset + search_pattern.len();
        let remaining = &full_text[val_start..obj_start + obj_end];
        let val_len = match remaining.find("</property>") {
            Some(l) => l,
            None => { writing.set(false); return; }
        };
        let val_end = val_start + val_len;
        if &full_text[val_start..val_end] == new_value {
            writing.set(false); return;
        }
        let cs = full_text[..val_start].chars().count();
        let ce = full_text[..val_end].chars().count();
        let mut si = buffer.iter_at_offset(cs as i32);
        let mut ei = buffer.iter_at_offset(ce as i32);
        buffer.begin_user_action();
        buffer.delete(&mut si, &mut ei);
        buffer.insert(&mut si, new_value);
        buffer.end_user_action();
    } else {
        // Property absent — insert before the closing </object> of this block.
        // Use rfind so we get the OUTER closing tag, not a nested one.
        let close_rel = match block.rfind("</object>") {
            Some(p) => p,
            None => { writing.set(false); return; }
        };
        let insert_byte = obj_start + close_rel;

        // Mirror the indentation of the closing tag
        let indent = full_text[..insert_byte]
            .rfind('\n')
            .map(|i| {
                let line_start = &full_text[i + 1..insert_byte];
                line_start.chars().take_while(|c| c.is_whitespace())
                    .collect::<String>()
            })
            .unwrap_or_default();

        let new_prop = format!(
            "{indent}<property name=\"{prop_name}\">{new_value}</property>\n"
        );
        let char_insert = full_text[..insert_byte].chars().count();
        let mut iter = buffer.iter_at_offset(char_insert as i32);
        buffer.begin_user_action();
        buffer.insert(&mut iter, &new_prop);
        buffer.end_user_action();
    }

    writing.set(false);
}

/// Write an edited property value back into the buffer, looking up the
/// property by name in the *current* buffer text to get fresh byte offsets.
/// This avoids stale-offset bugs when multiple properties are edited.
fn write_property_by_name(
    buffer: &sourceview5::Buffer,
    prop_name: &str,
    new_value: &str,
    writing: &Rc<Cell<bool>>,
) {
    // Prevent re-entry: buffer changes fire cursor-position-notify which
    // would rebuild the inspector while we're still modifying the buffer.
    if writing.get() {
        return;
    }
    writing.set(true);

    let (buf_start, buf_end) = buffer.bounds();
    let full_text = buffer.text(&buf_start, &buf_end, false).to_string();

    // Find the cursor position to locate which <object> we're editing
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let cursor_char = cursor.offset() as usize;
    let cursor_byte = full_text
        .char_indices()
        .nth(cursor_char)
        .map(|(i, _)| i)
        .unwrap_or(full_text.len());

    // Find the enclosing <object> block
    let safe_cursor = snap_to_char_boundary(&full_text, cursor_byte);
    let obj_start = match full_text[..safe_cursor].rfind("<object") {
        Some(s) => s,
        None => { writing.set(false); return; }
    };

    // Find matching </object>
    let obj_rest = &full_text[obj_start..];
    let obj_end = match find_closing_object(obj_rest) {
        Some(e) => e,
        None => { writing.set(false); return; }
    };
    let block = &full_text[obj_start..obj_start + obj_end];

    // Search for the property by name within this block
    let search_pattern = format!("<property name=\"{}\">", prop_name);
    let prop_offset = match block.find(&search_pattern) {
        Some(o) => o,
        None => { writing.set(false); return; }
    };

    let val_start_in_block = prop_offset + search_pattern.len();
    let remaining = &block[val_start_in_block..];
    let val_len = match remaining.find("</property>") {
        Some(l) => l,
        None => { writing.set(false); return; }
    };

    let abs_val_start = obj_start + val_start_in_block;
    let abs_val_end = abs_val_start + val_len;

    // Check if value actually changed
    let old_value = &full_text[abs_val_start..abs_val_end];
    if old_value == new_value {
        writing.set(false);
        return;
    }

    // Convert byte offsets to char offsets
    let char_start = full_text[..abs_val_start].chars().count();
    let char_end = full_text[..abs_val_end].chars().count();

    let mut start_iter = buffer.iter_at_offset(char_start as i32);
    let mut end_iter = buffer.iter_at_offset(char_end as i32);

    buffer.begin_user_action();
    buffer.delete(&mut start_iter, &mut end_iter);
    buffer.insert(&mut start_iter, new_value);
    buffer.end_user_action();

    writing.set(false);
}

/// Snap a byte offset to the nearest valid UTF-8 char boundary (rounding down).
fn snap_to_char_boundary(s: &str, offset: usize) -> usize {
    let offset = offset.min(s.len());
    let mut pos = offset;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Return the fixed set of valid values for well-known GTK4 enum properties,
/// or `None` if the property takes free-form text.
fn enum_options_for(prop_name: &str, current_value: &str) -> Option<&'static [&'static str]> {
    // Normalise property name (GTK uses both - and _ in .ui files)
    let name = prop_name.replace('-', "_");
    match name.as_str() {
        // ── Explicitly-named boolean properties ──────────────────────
        "visible" | "sensitive" | "can_focus" | "can_target" | "receives_default"
        | "focus_on_click" | "use_underline" | "use_markup" | "wrap" | "selectable"
        | "editable" | "activates_default" | "overwrite_mode" | "truncate_multiline"
        | "resizable" | "reorderable" | "show_tabs" | "scrollable" | "homogeneous"
        | "numeric" | "snap_to_ticks" | "has_frame" | "has_tooltip"
        | "accept_tab" | "cursor_visible" | "monospace" | "overlay_scrolling"
        | "enable_undo" | "show_border" | "no_show_all" | "expand"
        => Some(&["true", "false"]),

        // Catch any property whose live value is already a boolean
        _ if current_value == "true" || current_value == "false"
            => Some(&["true", "false"]),

        // ── Orientation ──────────────────────────────────────────────
        "orientation" => Some(&["horizontal", "vertical"]),

        // ── Alignment ────────────────────────────────────────────────
        "halign" | "valign" => Some(&["fill", "start", "center", "end", "baseline"]),

        // ── Justify ──────────────────────────────────────────────────
        "justify" => Some(&["left", "right", "center", "fill"]),

        // ── Wrap mode ────────────────────────────────────────────────
        "wrap_mode" => Some(&["none", "char", "word", "word_char"]),

        // ── Ellipsize ────────────────────────────────────────────────
        "ellipsize" => Some(&["none", "start", "middle", "end"]),

        // ── Scrollbar policy ─────────────────────────────────────────
        "hscrollbar_policy" | "vscrollbar_policy"
            => Some(&["always", "automatic", "never", "external"]),

        // ── Shadow / frame type ──────────────────────────────────────
        "shadow_type" => Some(&["none", "in", "out", "etched_in", "etched_out"]),

        // ── Notebook tab position ─────────────────────────────────────
        "tab_pos" => Some(&["top", "bottom", "left", "right"]),

        // ── Icon size ────────────────────────────────────────────────
        "icon_size" => Some(&["inherit", "normal", "large"]),

        // ── Pack type ────────────────────────────────────────────────
        "pack_type" => Some(&["start", "end"]),

        // ── Overflow ─────────────────────────────────────────────────
        "overflow" => Some(&["visible", "hidden"]),

        // ── Baseline position ─────────────────────────────────────────
        "baseline_position" => Some(&["top", "center", "bottom"]),

        // ── Selection mode ────────────────────────────────────────────
        "selection_mode" => Some(&["none", "single", "browse", "multiple"]),

        // ── Toolbar style ─────────────────────────────────────────────
        "toolbar_style" => Some(&["icons", "text", "both", "both_horiz"]),

        // ── Message type (InfoBar) ────────────────────────────────────
        "message_type" => Some(&["info", "warning", "question", "error", "other"]),

        // ── Response type ─────────────────────────────────────────────
        "response_type" => Some(&["none", "reject", "accept", "delete_event",
                                  "ok", "cancel", "close", "yes", "no", "apply", "help"]),

        // ── Button relief ─────────────────────────────────────────────
        "relief" => Some(&["normal", "none"]),

        // ── Text input purpose (Entry / SpinButton) ───────────────────
        "input_purpose" => Some(&["free_form", "alpha", "digits", "number",
                                  "phone", "url", "email", "name",
                                  "password", "pin", "terminal"]),

        // ── SpinButton update policy ──────────────────────────────────
        "update_policy" => Some(&["always", "if_valid"]),

        // ── Stack / Revealer transition type ──────────────────────────
        "transition_type" => Some(&["none", "crossfade",
                                    "slide_right", "slide_left",
                                    "slide_up", "slide_down",
                                    "slide_left_right", "slide_up_down",
                                    "over_up", "over_down",
                                    "over_left", "over_right",
                                    "under_up", "under_down",
                                    "under_left", "under_right",
                                    "over_up_down", "over_down_up",
                                    "over_left_right", "over_right_left",
                                    "rotate_left", "rotate_right",
                                    "rotate_left_right"]),

        // ── Picture content fit ───────────────────────────────────────
        "content_fit" => Some(&["fill", "contain", "cover", "scale_down"]),

        // ── Scrolled window corner placement ──────────────────────────
        "window_placement" => Some(&["top_left", "bottom_left", "top_right", "bottom_right"]),

        // ── Sort order ────────────────────────────────────────────────
        "sort_type" => Some(&["ascending", "descending"]),

        // ── Level bar mode ────────────────────────────────────────────
        "level_bar_mode" => Some(&["continuous", "discrete"]),

        // ── Size group mode ───────────────────────────────────────────
        "size_group_mode" => Some(&["none", "horizontal", "vertical", "both"]),

        _ => None,
    }
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
