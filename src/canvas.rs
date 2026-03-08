//! canvas.rs — Live preview pane for .ui files.
//!
//! Parses the XML text from the current editor buffer using `roxmltree`,
//! walks the node tree recursively, instantiates real GTK4 widgets,
//! applies `<property>` values, and wires `<child>` relationships to
//! compose the full widget hierarchy.
//!
//! Updates are debounced (500 ms after last keystroke) so we don't
//! re-parse on every character.

use gtk4::prelude::*;
use gtk4::{
    Adjustment, Box as GtkBox, Button, CenterBox, CheckButton, DropDown,
    Entry, Expander, Frame, GestureClick, Grid, HeaderBar, Image, Label,
    LevelBar, ListBox, Notebook, Orientation, Overlay, Paned,
    PasswordEntry, ProgressBar, Scale, ScrolledWindow, SearchEntry,
    Separator, SpinButton, Spinner, Switch, TextView, ToggleButton,
    Widget,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How long to wait (ms) after the last keystroke before re-rendering.
const DEBOUNCE_MS: u32 = 500;

/// Shared context passed through the recursive widget builder so click
/// handlers can move the editor cursor to the corresponding XML source.
#[derive(Clone)]
struct ClickCtx {
    /// The source buffer to move the cursor in.
    buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
    /// The XML string that was parsed (needed to convert byte→char offsets).
    xml: Rc<String>,
}

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
    /// The current source buffer (set via connect_buffer).
    source_buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
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
            source_buffer: Rc::new(RefCell::new(None)),
        }
    }

    /// Render a .ui XML string into the preview pane using roxmltree.
    pub fn render(&self, xml: &str) {
        // Remove previous child
        if let Some(old) = self.current_child.borrow_mut().take() {
            self.container.remove(&old);
        }

        if xml.trim().is_empty() {
            self.status.set_text("Empty buffer");
            return;
        }

        // Parse with roxmltree
        let doc = match roxmltree::Document::parse(xml) {
            Ok(d) => d,
            Err(e) => {
                self.status.set_text(&format!("XML error: {}", e));
                return;
            }
        };

        // Build click context for cursor-jump-on-click
        let ctx = ClickCtx {
            buffer: self.source_buffer.clone(),
            xml: Rc::new(xml.to_owned()),
        };

        // Find all top-level <object> nodes (may be wrapped in <interface>)
        let root = doc.root_element();
        let object_nodes: Vec<_> = if root.tag_name().name() == "interface" {
            root.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "object")
                .collect()
        } else if root.tag_name().name() == "object" {
            vec![root]
        } else {
            // Try to find any <object> descendant
            root.descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "object")
                .take(1)
                .collect()
        };

        if object_nodes.is_empty() {
            self.status.set_text("No <object> elements found");
            return;
        }

        // Build a container for all top-level objects
        let result_box = GtkBox::new(Orientation::Vertical, 8);
        result_box.set_margin_start(8);
        result_box.set_margin_end(8);
        result_box.set_margin_top(4);
        result_box.set_margin_bottom(8);

        let mut count = 0;
        for obj_node in &object_nodes {
            if let Some(w) = build_widget(*obj_node, &ctx) {
                let frame = Frame::new(None);
                frame.set_child(Some(&w));
                result_box.append(&frame);
                count += 1;
            }
        }

        if count == 0 {
            self.status.set_text("No renderable widgets found in .ui");
            return;
        }

        self.container.append(&result_box);
        *self.current_child.borrow_mut() = Some(result_box.upcast::<Widget>());
        self.status.set_text(&format!("OK — {} top-level widget{}", count, if count == 1 { "" } else { "s" }));
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
        // Store the buffer so click handlers can move the cursor
        *self.source_buffer.borrow_mut() = Some(buffer.clone());

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

// ─────────────────────────────────────────────────────────────────────
// roxmltree-based widget builder
// ─────────────────────────────────────────────────────────────────────

/// Collect all `<property>` children of a node into a Vec<(name, value)>.
fn collect_properties(node: roxmltree::Node) -> Vec<(String, String)> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "property")
        .filter_map(|prop| {
            let name = prop.attribute("name")?.to_string();
            // Property value can be text content or, for inline objects, we skip those
            let value = prop.text().unwrap_or("").to_string();
            Some((name, value))
        })
        .collect()
}

/// Find the first `<child>` → `<object>` inside a `<property>` node
/// (used for inline object properties like SpinButton's adjustment).
fn find_inline_object<'a, 'input>(prop_node: roxmltree::Node<'a, 'input>) -> Option<roxmltree::Node<'a, 'input>> {
    prop_node
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "object")
}

/// Collect all `<child>` elements of a node. Each `<child>` may contain
/// an `<object>` to recurse into. Returns (child_type, widget).
fn collect_children(node: roxmltree::Node, ctx: &ClickCtx) -> Vec<(Option<String>, Widget)> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "child")
        .filter_map(|child_node| {
            let child_type = child_node.attribute("type").map(|s| s.to_string());
            let obj = child_node
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "object")?;
            let widget = build_widget(obj, ctx)?;
            Some((child_type, widget))
        })
        .collect()
}

/// Look up a property value by name.
fn prop_val<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a str> {
    props.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// Parse a boolean property ("true", "1", "yes").
fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "true" | "1" | "yes")
}

/// Parse an orientation string.
fn parse_orientation(s: &str) -> Orientation {
    if s.trim() == "horizontal" {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    }
}

/// Parse a GtkAlign value.
fn parse_align(s: &str) -> gtk4::Align {
    match s.trim() {
        "start" => gtk4::Align::Start,
        "end" => gtk4::Align::End,
        "center" => gtk4::Align::Center,
        "fill" => gtk4::Align::Fill,
        "baseline" => gtk4::Align::Baseline,
        _ => gtk4::Align::Fill,
    }
}

/// Apply properties common to all widgets.
fn apply_common_props(w: &impl WidgetExt, props: &[(String, String)]) {
    if let Some(v) = prop_val(props, "hexpand") {
        w.set_hexpand(parse_bool(v));
    }
    if let Some(v) = prop_val(props, "vexpand") {
        w.set_vexpand(parse_bool(v));
    }
    if let Some(v) = prop_val(props, "halign") {
        w.set_halign(parse_align(v));
    }
    if let Some(v) = prop_val(props, "valign") {
        w.set_valign(parse_align(v));
    }
    if let Some(v) = prop_val(props, "visible") {
        w.set_visible(parse_bool(v));
    }
    if let Some(v) = prop_val(props, "sensitive") {
        w.set_sensitive(parse_bool(v));
    }
    if let Some(v) = prop_val(props, "width-request") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_width_request(n);
        }
    }
    if let Some(v) = prop_val(props, "height-request") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_height_request(n);
        }
    }
    if let Some(v) = prop_val(props, "margin-start") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_margin_start(n);
        }
    }
    if let Some(v) = prop_val(props, "margin-end") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_margin_end(n);
        }
    }
    if let Some(v) = prop_val(props, "margin-top") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_margin_top(n);
        }
    }
    if let Some(v) = prop_val(props, "margin-bottom") {
        if let Ok(n) = v.trim().parse::<i32>() {
            w.set_margin_bottom(n);
        }
    }
    if let Some(v) = prop_val(props, "tooltip-text") {
        w.set_tooltip_text(Some(v));
    }
    if let Some(v) = prop_val(props, "css-classes") {
        for cls in v.split_whitespace() {
            w.add_css_class(cls);
        }
    }
}

/// Build an Adjustment from an inline <object class="GtkAdjustment"> node.
fn build_adjustment(node: roxmltree::Node) -> Adjustment {
    let props = collect_properties(node);
    let lower = prop_val(&props, "lower")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let upper = prop_val(&props, "upper")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(100.0);
    let step = prop_val(&props, "step-increment")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(1.0);
    let page = prop_val(&props, "page-increment")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(10.0);
    let page_size = prop_val(&props, "page-size")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let value = prop_val(&props, "value")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(lower);
    Adjustment::new(value, lower, upper, step, page, page_size)
}

/// Try to find an inline GtkAdjustment in a <property name="adjustment"> node.
fn find_adjustment<'a, 'input>(node: roxmltree::Node<'a, 'input>) -> Option<Adjustment> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "property")
        .find(|n| n.attribute("name") == Some("adjustment"))
        .and_then(find_inline_object)
        .filter(|obj: &roxmltree::Node| obj.attribute("class").unwrap_or("") == "GtkAdjustment")
        .map(build_adjustment)
}

/// Collect string items from a GtkDropDown's inline GtkStringList model.
/// Looks for: <property name="model"><object class="GtkStringList"><items><item>...</item>...
fn collect_string_list_items(node: roxmltree::Node) -> Vec<String> {
    // Find <property name="model"> → <object class="GtkStringList"> → <items> → <item>
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "property")
        .find(|n| n.attribute("name") == Some("model"))
        .and_then(|prop| {
            prop.children()
                .find(|n| n.is_element() && n.tag_name().name() == "object"
                    && n.attribute("class") == Some("GtkStringList"))
        })
        .map(|string_list| {
            string_list.descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "item")
                .filter_map(|item| item.text().map(|t| t.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Collect items from a GtkComboBoxText's <items> element.
/// Looks for: <items><item>...</item>...
fn collect_combobox_items(node: roxmltree::Node) -> Vec<String> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == "items")
        .map(|items| {
            items.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "item")
                .filter_map(|item| item.text().map(|t| t.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Attach a click gesture to a widget that jumps the editor cursor to
/// the `<object>` tag's position in the source XML when clicked.
/// For GtkExpander, also toggles expanded state since we claim the click.
fn attach_click_to_select(widget: &Widget, byte_offset: usize, ctx: &ClickCtx, class: &str) {
    let gesture = GestureClick::new();
    gesture.set_button(1); // left click

    let buf_ref = ctx.buffer.clone();
    let xml_rc = ctx.xml.clone();
    let is_expander = class == "GtkExpander";
    let widget_ref = widget.clone();

    gesture.connect_pressed(move |gesture, _n_press, _x, _y| {
        // Stop propagation so clicking a leaf widget doesn't also fire
        // on every ancestor container
        gesture.set_state(gtk4::EventSequenceState::Claimed);

        // For Expander: toggle expand/collapse since we claimed the click
        // that would normally do this
        if is_expander {
            if let Some(exp) = widget_ref.downcast_ref::<Expander>() {
                exp.set_expanded(!exp.is_expanded());
            }
        }

        if let Some(buffer) = buf_ref.borrow().as_ref() {
            // Convert byte offset → char offset (safe for multi-byte UTF-8)
            let xml = xml_rc.as_str();
            let safe_offset = byte_offset.min(xml.len());
            let char_offset = xml[..safe_offset].chars().count();

            // Move the editor cursor
            let iter = buffer.iter_at_offset(char_offset as i32);
            buffer.place_cursor(&iter);

            // cursor-position-notify fires → inspector updates automatically
        }
    });

    widget.add_controller(gesture);
}

/// Recursively build a GTK widget from an `<object>` node.
fn build_widget(node: roxmltree::Node, ctx: &ClickCtx) -> Option<Widget> {
    let class = node.attribute("class")?;
    let props = collect_properties(node);
    let children = collect_children(node, ctx);

    let widget: Widget = match class {
        // ── Containers ──────────────────────────────────────────────
        "GtkBox" => {
            let orientation = prop_val(&props, "orientation")
                .map(parse_orientation)
                .unwrap_or(Orientation::Horizontal);
            let spacing = prop_val(&props, "spacing")
                .and_then(|v| v.trim().parse::<i32>().ok())
                .unwrap_or(0);
            let b = GtkBox::new(orientation, spacing);
            apply_common_props(&b, &props);
            for (_ty, child) in &children {
                b.append(child);
            }
            b.upcast()
        }

        "GtkGrid" => {
            let g = Grid::new();
            if let Some(v) = prop_val(&props, "row-spacing") {
                if let Ok(n) = v.trim().parse::<u32>() { g.set_row_spacing(n); }
            }
            if let Some(v) = prop_val(&props, "column-spacing") {
                if let Ok(n) = v.trim().parse::<u32>() { g.set_column_spacing(n); }
            }
            apply_common_props(&g, &props);
            // Attach children — use <layout> properties if present, else auto-stack
            let mut auto_row = 0i32;
            for (_ty, child) in &children {
                // Try to read layout col/row from the child's own node
                // For now, auto-place in a vertical column
                g.attach(child, 0, auto_row, 1, 1);
                auto_row += 1;
            }
            g.upcast()
        }

        "GtkFrame" => {
            let label = prop_val(&props, "label");
            let f = Frame::new(label);
            apply_common_props(&f, &props);
            if let Some((_ty, child)) = children.first() {
                f.set_child(Some(child));
            }
            f.upcast()
        }

        "GtkScrolledWindow" => {
            let sw = ScrolledWindow::new();
            apply_common_props(&sw, &props);
            if let Some((_ty, child)) = children.first() {
                sw.set_child(Some(child));
            }
            sw.upcast()
        }

        "GtkPaned" => {
            let orientation = prop_val(&props, "orientation")
                .map(parse_orientation)
                .unwrap_or(Orientation::Horizontal);
            let p = Paned::new(orientation);
            apply_common_props(&p, &props);
            if let Some((_ty, c)) = children.get(0) {
                p.set_start_child(Some(c));
            }
            if let Some((_ty, c)) = children.get(1) {
                p.set_end_child(Some(c));
            }
            p.upcast()
        }

        "GtkNotebook" => {
            let nb = Notebook::new();
            apply_common_props(&nb, &props);
            for (_ty, child) in &children {
                nb.append_page(child, None::<&Label>);
            }
            nb.upcast()
        }

        "GtkStack" => {
            let stack = gtk4::Stack::new();
            apply_common_props(&stack, &props);
            for (_ty, child) in &children {
                stack.add_child(child);
            }
            stack.upcast()
        }

        "GtkOverlay" => {
            let overlay = Overlay::new();
            apply_common_props(&overlay, &props);
            let mut first = true;
            for (_ty, child) in &children {
                if first {
                    overlay.set_child(Some(child));
                    first = false;
                } else {
                    overlay.add_overlay(child);
                }
            }
            overlay.upcast()
        }

        "GtkCenterBox" => {
            let cb = CenterBox::new();
            apply_common_props(&cb, &props);
            for (ty, child) in &children {
                match ty.as_deref() {
                    Some("start") => cb.set_start_widget(Some(child)),
                    Some("end") => cb.set_end_widget(Some(child)),
                    _ => cb.set_center_widget(Some(child)),
                }
            }
            cb.upcast()
        }

        "GtkExpander" => {
            let label = prop_val(&props, "label").unwrap_or("Expander");
            let e = Expander::new(Some(label));
            apply_common_props(&e, &props);
            if let Some((_ty, child)) = children.first() {
                e.set_child(Some(child));
            }
            e.upcast()
        }

        "GtkFlowBox" => {
            let fb = gtk4::FlowBox::new();
            apply_common_props(&fb, &props);
            if let Some(v) = prop_val(&props, "max-children-per-line") {
                if let Ok(n) = v.trim().parse::<u32>() { fb.set_max_children_per_line(n); }
            }
            if let Some(v) = prop_val(&props, "min-children-per-line") {
                if let Ok(n) = v.trim().parse::<u32>() { fb.set_min_children_per_line(n); }
            }
            for (_ty, child) in &children {
                fb.append(child);
            }
            fb.upcast()
        }

        "GtkListBox" => {
            let lb = ListBox::new();
            apply_common_props(&lb, &props);
            for (_ty, child) in &children {
                lb.append(child);
            }
            lb.upcast()
        }

        // ── Display ─────────────────────────────────────────────────
        "GtkLabel" => {
            let text = prop_val(&props, "label").unwrap_or("Label");
            let l = Label::new(Some(text));
            apply_common_props(&l, &props);
            if let Some(v) = prop_val(&props, "wrap") {
                l.set_wrap(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "selectable") {
                l.set_selectable(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "use-markup") {
                l.set_use_markup(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "xalign") {
                if let Ok(n) = v.trim().parse::<f32>() { l.set_xalign(n); }
            }
            l.upcast()
        }

        "GtkImage" => {
            let img = Image::new();
            apply_common_props(&img, &props);
            if let Some(v) = prop_val(&props, "icon-name") {
                img.set_icon_name(Some(v));
            }
            if let Some(v) = prop_val(&props, "pixel-size") {
                if let Ok(n) = v.trim().parse::<i32>() { img.set_pixel_size(n); }
            }
            img.upcast()
        }

        "GtkSeparator" => {
            let orientation = prop_val(&props, "orientation")
                .map(parse_orientation)
                .unwrap_or(Orientation::Horizontal);
            let s = Separator::new(orientation);
            apply_common_props(&s, &props);
            s.upcast()
        }

        "GtkProgressBar" => {
            let pb = ProgressBar::new();
            apply_common_props(&pb, &props);
            if let Some(v) = prop_val(&props, "fraction") {
                if let Ok(n) = v.trim().parse::<f64>() { pb.set_fraction(n); }
            }
            if let Some(v) = prop_val(&props, "show-text") {
                pb.set_show_text(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "text") {
                pb.set_text(Some(v));
            }
            pb.upcast()
        }

        "GtkSpinner" => {
            let sp = Spinner::new();
            apply_common_props(&sp, &props);
            if let Some(v) = prop_val(&props, "spinning") {
                sp.set_spinning(parse_bool(v));
            }
            sp.upcast()
        }

        "GtkLevelBar" => {
            let lb = LevelBar::new();
            apply_common_props(&lb, &props);
            if let Some(v) = prop_val(&props, "value") {
                if let Ok(n) = v.trim().parse::<f64>() { lb.set_value(n); }
            }
            if let Some(v) = prop_val(&props, "min-value") {
                if let Ok(n) = v.trim().parse::<f64>() { lb.set_min_value(n); }
            }
            if let Some(v) = prop_val(&props, "max-value") {
                if let Ok(n) = v.trim().parse::<f64>() { lb.set_max_value(n); }
            }
            lb.upcast()
        }

        // ── Input ───────────────────────────────────────────────────
        "GtkButton" => {
            let btn = Button::new();
            apply_common_props(&btn, &props);
            if let Some(v) = prop_val(&props, "label") {
                btn.set_label(v);
            }
            if let Some(v) = prop_val(&props, "icon-name") {
                btn.set_icon_name(v);
            }
            // If the button has a child widget, use that instead
            if let Some((_ty, child)) = children.first() {
                btn.set_child(Some(child));
            }
            btn.upcast()
        }

        "GtkToggleButton" => {
            let btn = ToggleButton::new();
            apply_common_props(&btn, &props);
            if let Some(v) = prop_val(&props, "label") {
                btn.set_label(v);
            }
            if let Some(v) = prop_val(&props, "active") {
                btn.set_active(parse_bool(v));
            }
            btn.upcast()
        }

        "GtkCheckButton" => {
            let cb = CheckButton::new();
            apply_common_props(&cb, &props);
            if let Some(v) = prop_val(&props, "label") {
                cb.set_label(Some(v));
            }
            if let Some(v) = prop_val(&props, "active") {
                cb.set_active(parse_bool(v));
            }
            cb.upcast()
        }

        "GtkSwitch" => {
            let sw = Switch::new();
            apply_common_props(&sw, &props);
            if let Some(v) = prop_val(&props, "active") {
                sw.set_active(parse_bool(v));
            }
            sw.upcast()
        }

        "GtkEntry" => {
            let e = Entry::new();
            apply_common_props(&e, &props);
            if let Some(v) = prop_val(&props, "placeholder-text") {
                e.set_placeholder_text(Some(v));
            }
            if let Some(v) = prop_val(&props, "text") {
                e.set_text(v);
            }
            if let Some(v) = prop_val(&props, "max-length") {
                if let Ok(n) = v.trim().parse::<i32>() { e.set_max_length(n); }
            }
            e.upcast()
        }

        "GtkPasswordEntry" => {
            let pe = PasswordEntry::new();
            apply_common_props(&pe, &props);
            if let Some(v) = prop_val(&props, "show-peek-icon") {
                pe.set_show_peek_icon(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "placeholder-text") {
                pe.set_placeholder_text(Some(v));
            }
            pe.upcast()
        }

        "GtkSpinButton" => {
            let adj = find_adjustment(node).unwrap_or_else(|| {
                Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 0.0)
            });
            let climb_rate = prop_val(&props, "climb-rate")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .unwrap_or(1.0);
            let digits = prop_val(&props, "digits")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let sb = SpinButton::new(Some(&adj), climb_rate, digits);
            apply_common_props(&sb, &props);
            sb.upcast()
        }

        "GtkScale" => {
            let orientation = prop_val(&props, "orientation")
                .map(parse_orientation)
                .unwrap_or(Orientation::Horizontal);
            let adj = find_adjustment(node).unwrap_or_else(|| {
                Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 0.0)
            });
            let s = Scale::new(orientation, Some(&adj));
            apply_common_props(&s, &props);
            if let Some(v) = prop_val(&props, "draw-value") {
                s.set_draw_value(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "digits") {
                if let Ok(n) = v.trim().parse::<i32>() { s.set_digits(n); }
            }
            s.upcast()
        }

        "GtkDropDown" => {
            let items = collect_string_list_items(node);
            let strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            let dd = DropDown::from_strings(&strs);
            apply_common_props(&dd, &props);
            dd.upcast()
        }

        "GtkComboBoxText" => {
            let items = collect_combobox_items(node);
            let strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            // ComboBoxText is deprecated in 4.10, render as DropDown
            let dd = DropDown::from_strings(&strs);
            apply_common_props(&dd, &props);
            dd.upcast()
        }

        // ── Text ────────────────────────────────────────────────────
        "GtkTextView" => {
            let tv = TextView::new();
            apply_common_props(&tv, &props);
            if let Some(v) = prop_val(&props, "editable") {
                tv.set_editable(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "wrap-mode") {
                let mode = match v.trim() {
                    "none" => gtk4::WrapMode::None,
                    "char" => gtk4::WrapMode::Char,
                    "word" => gtk4::WrapMode::Word,
                    "word-char" => gtk4::WrapMode::WordChar,
                    _ => gtk4::WrapMode::None,
                };
                tv.set_wrap_mode(mode);
            }
            tv.upcast()
        }

        "GtkSearchEntry" => {
            let se = SearchEntry::new();
            apply_common_props(&se, &props);
            if let Some(v) = prop_val(&props, "placeholder-text") {
                se.set_placeholder_text(Some(v));
            }
            se.upcast()
        }

        // ── Layout ──────────────────────────────────────────────────
        "GtkHeaderBar" => {
            let hb = HeaderBar::new();
            apply_common_props(&hb, &props);
            for (ty, child) in &children {
                match ty.as_deref() {
                    Some("start") => hb.pack_start(child),
                    Some("end") => hb.pack_end(child),
                    _ => hb.pack_start(child),
                }
            }
            hb.upcast()
        }

        "GtkActionBar" => {
            let ab = gtk4::ActionBar::new();
            apply_common_props(&ab, &props);
            for (ty, child) in &children {
                match ty.as_deref() {
                    Some("start") => ab.pack_start(child),
                    Some("end") => ab.pack_end(child),
                    _ => ab.pack_start(child),
                }
            }
            ab.upcast()
        }

        // ── Special internal types ─────────────────────────────────
        // GtkStackPage, GtkNotebookPage — unwrap and return child
        "GtkStackPage" | "GtkNotebookPage" => {
            // These are container wrappers; look for a "child" property
            // that contains an inline <object>
            let child_prop = node.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "property")
                .find(|n| n.attribute("name") == Some("child"));
            if let Some(cp) = child_prop {
                if let Some(obj) = find_inline_object(cp) {
                    return build_widget(obj, ctx);
                }
            }
            // Fallback: try direct children
            if let Some((_ty, child)) = children.first() {
                return Some(child.clone());
            }
            None?
        }

        "GtkAdjustment" | "GtkStringList" => {
            // Non-visual helper objects — skip
            return None;
        }

        // ── Fallback ────────────────────────────────────────────────
        _ => {
            // Unknown widget — show a placeholder
            let placeholder = Label::new(Some(&format!("[{class}]")));
            placeholder.add_css_class("dim-label");
            apply_common_props(&placeholder, &props);
            placeholder.upcast()
        }
    };

    // Attach click-to-select: clicking this widget in the canvas jumps
    // the editor cursor to the <object> tag in the source XML.
    // Offset past "<object" (7 bytes) so the cursor lands INSIDE the tag —
    // the inspector's rfind("<object") then finds THIS node, not the parent.
    let byte_offset = node.range().start + 7;
    attach_click_to_select(&widget, byte_offset, ctx, class);

    Some(widget)
}
