//! toolbox.rs — Combined Widget Palette + Property Inspector panel.
//!
//! A vertical Paned with the palette (widget picker) on top and the
//! property inspector on the bottom. One panel, one toggle, one shortcut.

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation, Paned};

use crate::inspector::Inspector;
use crate::palette::Palette;

/// The Toolbox panel — palette on top, inspector on bottom.
#[derive(Clone)]
pub struct Toolbox {
    pub widget: GtkBox,
    pub palette: Palette,
    pub inspector: Inspector,
}

impl Toolbox {
    pub fn new() -> Self {
        let palette = Palette::new();
        let inspector = Inspector::new();

        // Remove the individual width requests — the toolbox controls width
        palette.widget.set_width_request(-1);
        inspector.widget.set_width_request(-1);

        let paned = Paned::new(Orientation::Vertical);
        paned.set_start_child(Some(&palette.widget));
        paned.set_end_child(Some(&inspector.widget));
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);

        // Set 50/50 split once mapped
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
        widget.append(&paned);

        // Both children visible inside the toolbox
        palette.widget.set_visible(true);
        inspector.widget.set_visible(true);

        Toolbox { widget, palette, inspector }
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
