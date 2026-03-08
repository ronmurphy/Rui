//! canvas.rs — Live preview pane for .ui files.
//!
//! Takes the XML text from the current editor buffer, passes it through
//! `gtk4::Builder::from_string()`, extracts the first top-level object,
//! and renders it inside a scrollable container.
//!
//! Updates are debounced (500 ms after last keystroke) so we don't
//! re-parse on every character.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Builder, Frame, Label, Orientation, Overlay,
    ScrolledWindow, Widget,
};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How long to wait (ms) after the last keystroke before re-rendering.
const DEBOUNCE_MS: u32 = 500;

/// The live preview panel shown beside the editor.
#[derive(Clone)]
pub struct Canvas {
    /// The outermost widget to pack into a Paned.
    pub widget: GtkBox,
    /// Where the rendered preview goes.
    container: GtkBox,
    /// Error/status label (shown when XML is invalid).
    status: Label,
    /// Currently rendered child (so we can remove it before adding a new one).
    current_child: Rc<RefCell<Option<Widget>>>,
    /// Debounce generation counter.
    generation: Rc<Cell<u64>>,
}

impl Canvas {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 0);
        container.set_hexpand(true);
        container.set_vexpand(true);

        let status = Label::new(Some("No .ui file open"));
        status.set_halign(gtk4::Align::Start);
        status.set_margin_start(8);
        status.set_margin_end(8);
        status.set_margin_top(4);
        status.set_margin_bottom(4);
        status.add_css_class("dim-label");
        status.set_wrap(true);
        status.set_selectable(true);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&container)
            .build();

        let header = Label::new(Some("Preview"));
        header.set_halign(gtk4::Align::Start);
        header.set_margin_start(8);
        header.set_margin_top(4);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(300);
        widget.append(&header);
        widget.append(&status);
        widget.append(&scroll);

        Canvas {
            widget,
            container,
            status,
            current_child: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
        }
    }

    /// Render a .ui XML string into the preview pane.
    pub fn render(&self, xml: &str) {
        // Remove previous child
        if let Some(old) = self.current_child.borrow_mut().take() {
            self.container.remove(&old);
        }

        if xml.trim().is_empty() {
            self.status.set_text("Empty buffer");
            return;
        }

        // Try to parse the XML with GtkBuilder
        let builder = Builder::new();
        // We use add_from_string which returns a Result
        match builder.add_from_string(xml) {
            Ok(_) => {}
            Err(e) => {
                self.status.set_text(&format!("XML error: {}", e.message()));
                return;
            }
        }

        // Get all top-level objects and find the first widget
        let objects = builder.objects();
        let first_widget = objects.iter().find_map(|obj| {
            obj.clone().downcast::<Widget>().ok()
        });

        match first_widget {
            Some(w) => {
                // Wrap in a Frame so it has a visible boundary
                let frame = Frame::new(None);
                frame.set_margin_start(8);
                frame.set_margin_end(8);
                frame.set_margin_top(4);
                frame.set_margin_bottom(8);
                frame.set_child(Some(&w));

                self.container.append(&frame);
                *self.current_child.borrow_mut() = Some(frame.upcast::<Widget>());
                self.status.set_text("OK");
            }
            None => {
                self.status.set_text("No renderable widget found in .ui");
            }
        }
    }

    /// Clear the preview pane.
    pub fn clear(&self) {
        if let Some(old) = self.current_child.borrow_mut().take() {
            self.container.remove(&old);
        }
        self.status.set_text("No .ui file open");
    }

    /// Connect to a sourceview5 Buffer so the preview auto-updates
    /// whenever the text changes (debounced via generation counter).
    pub fn connect_buffer(&self, buffer: &sourceview5::Buffer) {
        let canvas = self.clone();
        let buf = buffer.clone();

        buffer.connect_changed(move |_| {
            // Bump generation — any pending timer with an older gen becomes a no-op.
            let gen = canvas.generation.get().wrapping_add(1);
            canvas.generation.set(gen);

            let canvas2 = canvas.clone();
            let buf2 = buf.clone();
            let expected_gen = canvas.generation.clone();

            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(DEBOUNCE_MS as u64),
                move || {
                    // Only render if no newer change has occurred.
                    if expected_gen.get() == gen {
                        let (start, end) = buf2.bounds();
                        let text = buf2.text(&start, &end, false).to_string();
                        canvas2.render(&text);
                    }
                },
            );
        });
    }

    /// Returns true if a .ui file is appropriate for preview.
    pub fn is_ui_file(path: &std::path::Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("ui" | "xml")
        )
    }

    pub fn toggle(&self) {
        self.widget.set_visible(!self.widget.is_visible());
    }

    pub fn is_visible(&self) -> bool {
        self.widget.is_visible()
    }
}
