//! toolbox.rs — Combined Widget Palette + Property Inspector panel.
//!
//! A vertical Paned with the palette (widget picker) on top and the
//! property inspector on the bottom. One panel, one toggle, one shortcut.
//!
//! Between the header and the paned sits a VB6-style widget selector:
//! a DropDown listing every widget in the active .ui file, with a trash
//! button to delete the currently selected widget.

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, DropDown, Label, Orientation, Paned, StringList};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::inspector::Inspector;
use crate::outline::SelectorItem;
use crate::palette::Palette;

/// The Toolbox panel — selector combo on top, palette + inspector in a Paned below.
#[derive(Clone)]
pub struct Toolbox {
    pub widget:    GtkBox,
    pub palette:   Palette,
    pub inspector: Inspector,
    /// The DropDown widget (kept for set_model on repopulate).
    selector_dd:           DropDown,
    /// Parallel vecs indexed by combo position.
    selector_offsets:      Rc<RefCell<Vec<usize>>>,
    selector_child_ranges: Rc<RefCell<Vec<Option<(usize, usize)>>>>,
    /// Suppresses the selected-notify signal while we repopulate the model.
    is_populating:         Rc<Cell<bool>>,
    on_select_cb:          Rc<RefCell<Option<Box<dyn Fn(usize)>>>>,
    on_delete_cb:          Rc<RefCell<Option<Box<dyn Fn(usize, usize)>>>>,
}

impl Toolbox {
    pub fn new() -> Self {
        let palette   = Palette::new();
        let inspector = Inspector::new();

        palette.widget.set_width_request(-1);
        inspector.widget.set_width_request(-1);

        // ── VB6 selector row ────────────────────────────────────────────────
        let selector_offsets:      Rc<RefCell<Vec<usize>>>                 = Rc::new(RefCell::new(Vec::new()));
        let selector_child_ranges: Rc<RefCell<Vec<Option<(usize, usize)>>>> = Rc::new(RefCell::new(Vec::new()));
        let is_populating = Rc::new(Cell::new(false));
        let on_select_cb:  Rc<RefCell<Option<Box<dyn Fn(usize)>>>>           = Rc::new(RefCell::new(None));
        let on_delete_cb:  Rc<RefCell<Option<Box<dyn Fn(usize, usize)>>>>    = Rc::new(RefCell::new(None));

        let initial_model = StringList::new(&[]);
        let selector_dd   = DropDown::builder()
            .model(&initial_model)
            .hexpand(true)
            .tooltip_text("Select widget")
            .build();

        // Trash button beside the combo.
        let trash_lbl = Label::new(Some("\u{F1F8}")); // nf-fa-trash
        trash_lbl.add_css_class("nf");
        let trash_btn = Button::new();
        trash_btn.set_child(Some(&trash_lbl));
        trash_btn.add_css_class("flat");
        trash_btn.set_valign(gtk4::Align::Center);
        trash_btn.set_tooltip_text(Some("Delete selected widget"));

        let sel_row = GtkBox::new(Orientation::Horizontal, 4);
        sel_row.set_margin_start(6);
        sel_row.set_margin_end(6);
        sel_row.set_margin_top(4);
        sel_row.set_margin_bottom(4);
        sel_row.append(&selector_dd);
        sel_row.append(&trash_btn);

        // Wire combo selection → on_select_cb.
        {
            let is_pop    = is_populating.clone();
            let offsets   = selector_offsets.clone();
            let cb_ref    = on_select_cb.clone();
            selector_dd.connect_selected_notify(move |dd| {
                if is_pop.get() { return; }
                let idx = dd.selected();
                if idx == gtk4::INVALID_LIST_POSITION { return; }
                let offsets = offsets.borrow();
                if let Some(&byte_offset) = offsets.get(idx as usize) {
                    if let Some(cb) = cb_ref.borrow().as_ref() {
                        cb(byte_offset);
                    }
                }
            });
        }

        // Wire trash button → on_delete_cb.
        {
            let dd_ref  = selector_dd.clone();
            let ranges  = selector_child_ranges.clone();
            let cb_ref  = on_delete_cb.clone();
            trash_btn.connect_clicked(move |_| {
                let idx = dd_ref.selected();
                if idx == gtk4::INVALID_LIST_POSITION { return; }
                let ranges = ranges.borrow();
                if let Some(Some((start, end))) = ranges.get(idx as usize) {
                    if let Some(cb) = cb_ref.borrow().as_ref() {
                        cb(*start, *end);
                    }
                }
            });
        }

        // ── Palette + Inspector paned ────────────────────────────────────────
        let paned = Paned::new(Orientation::Vertical);
        paned.set_start_child(Some(&palette.widget));
        paned.set_end_child(Some(&inspector.widget));
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);

        {
            let p = paned.clone();
            paned.connect_map(move |_| {
                let h = p.allocated_height();
                if h > 0 {
                    p.set_position(h / 2);
                }
            });
        }

        let header = Label::new(Some("Toolbox"));
        header.set_halign(gtk4::Align::Start);
        header.set_margin_start(8);
        header.set_margin_top(4);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(220);
        widget.append(&header);
        widget.append(&sel_row);
        widget.append(&paned);

        palette.widget.set_visible(true);
        inspector.widget.set_visible(true);

        Toolbox {
            widget,
            palette,
            inspector,
            selector_dd,
            selector_offsets,
            selector_child_ranges,
            is_populating,
            on_select_cb,
            on_delete_cb,
        }
    }

    /// Repopulate the widget selector combo from a fresh outline rebuild.
    pub fn populate_selector(&self, items: Vec<SelectorItem>) {
        self.is_populating.set(true);

        let strings: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        let str_refs: Vec<&str>  = strings.iter().map(String::as_str).collect();
        let new_model = StringList::new(&str_refs);
        self.selector_dd.set_model(Some(&new_model));

        *self.selector_offsets.borrow_mut()      = items.iter().map(|i| i.byte_offset).collect();
        *self.selector_child_ranges.borrow_mut() = items.iter().map(|i| i.child_range).collect();

        self.is_populating.set(false);
    }

    /// Called when the user picks an entry from the selector combo.
    /// Receives the `byte_offset` of the chosen widget (for `Canvas::select_from_tree`).
    pub fn on_select_widget<F: Fn(usize) + 'static>(&self, cb: F) {
        *self.on_select_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Called when the trash button is clicked.
    /// Receives `(child_start, child_end)` byte range of the widget to delete.
    pub fn on_delete_widget<F: Fn(usize, usize) + 'static>(&self, cb: F) {
        *self.on_delete_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Set the buffer for both palette (snippet insertion) and inspector (property editing).
    pub fn set_buffer(&self, buffer: &sourceview5::Buffer) {
        self.palette.set_buffer(buffer);
    }

    /// Connect the inspector to a .ui buffer (hooks cursor-position-notify).
    /// Also sets the palette buffer for snippet insertion.
    pub fn connect_buffer(&self, buffer: &sourceview5::Buffer) {
        self.palette.set_buffer(buffer);
        self.inspector.connect_buffer(buffer);
    }

    /// Show/hide the toolbox.
    pub fn toggle(&self) {
        self.widget.set_visible(!self.widget.is_visible());
    }
}
