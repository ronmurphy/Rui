//! toolbox.rs — Combined Widget Palette + Property Inspector panel.
//!
//! A vertical Paned with the palette (widget picker) on top and the
//! property inspector on the bottom. One panel, one toggle, one shortcut.
//!
//! Between the header and the paned sits a VB6-style widget selector:
//! a DropDown listing every widget in the active .ui file, with a trash
//! button to delete the currently selected widget.

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation};

use crate::inspector::Inspector;
use crate::palette::Palette;

/// The Toolbox panel — palette on top, inspector below.
#[derive(Clone)]
pub struct Toolbox {
    pub widget:    GtkBox,
    pub palette:   Palette,
    pub inspector: Inspector,
}

impl Toolbox {
    pub fn new() -> Self {
        let palette   = Palette::new();
        let inspector = Inspector::new();

        palette.widget.set_width_request(-1);
        inspector.widget.set_width_request(-1);

        let header = Label::new(Some("Toolbox"));
        header.set_halign(gtk4::Align::Start);
        header.set_margin_start(8);
        header.set_margin_top(4);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(-1);
        widget.append(&header);

        palette.widget.set_vexpand(true);
        widget.append(&palette.widget);

        Toolbox { widget, palette, inspector }
    }

    /// Set the buffer for palette (snippet insertion).
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
