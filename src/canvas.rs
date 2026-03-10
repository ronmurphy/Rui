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
    Adjustment, Box as GtkBox, Button, CenterBox, CheckButton,
    DragSource, DropDown, DropTarget, Entry, Expander, Frame, GestureClick,
    Grid, HeaderBar, Image, Label, LevelBar, ListBox, Notebook, Orientation,
    Overlay, Paned, PasswordEntry, Popover, ProgressBar, Scale, ScrolledWindow,
    SearchEntry, Separator, SpinButton, Spinner, Switch, TextView, ToggleButton,
    Widget,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How long to wait (ms) after the last keystroke before re-rendering.
const DEBOUNCE_MS: u32 = 500;

/// Shared context passed through the recursive widget builder so click
/// handlers can move the editor cursor to the corresponding XML source.
/// Callback type for double-click on an interactive widget.
/// Parameters: (class_name, widget_id)
type DblClickCb = Rc<RefCell<Option<Box<dyn Fn(&str, &str)>>>>;

#[derive(Clone)]
struct ClickCtx {
    /// The source buffer to move the cursor in.
    buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
    /// The XML string that was parsed (needed to convert byte→char offsets).
    xml: Rc<String>,
    /// Optional double-click callback for codegen.
    on_double_click: DblClickCb,
    /// Currently selected widget — used to show/hide the selection outline.
    selected_widget: Rc<RefCell<Option<Widget>>>,
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
    /// Double-click callback: (class, id) → open/create companion file.
    on_double_click: DblClickCb,
}

impl Canvas {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 0);
        container.set_hexpand(true);
        container.set_vexpand(true);

        let status = Label::new(Some("Open a .ui file or create one via File → New .ui File"));
        status.set_halign(gtk4::Align::Center);
        status.set_valign(gtk4::Align::Center);
        status.set_vexpand(true);
        status.set_margin_start(24);
        status.set_margin_end(24);
        status.set_margin_top(8);
        status.set_margin_bottom(8);
        status.add_css_class("dim-label");
        status.set_wrap(true);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&container)
            .build();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.append(&status);
        widget.append(&scroll);

        Canvas {
            widget,
            container,
            status,
            current_child: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
            source_buffer: Rc::new(RefCell::new(None)),
            on_double_click: Rc::new(RefCell::new(None)),
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
            self.status.set_visible(true);
            self.status.set_vexpand(true);
            return;
        }

        // Parse with roxmltree
        let doc = match roxmltree::Document::parse(xml) {
            Ok(d) => d,
            Err(e) => {
                self.status.set_text(&format!("XML error: {}", e));
                self.status.set_visible(true);
                self.status.set_vexpand(true);
                return;
            }
        };

        // Build click context for cursor-jump-on-click
        let ctx = ClickCtx {
            buffer: self.source_buffer.clone(),
            xml: Rc::new(xml.to_owned()),
            on_double_click: self.on_double_click.clone(),
            selected_widget: Rc::new(RefCell::new(None)),
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
            self.status.set_visible(true);
            self.status.set_vexpand(true);
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
            self.status.set_visible(true);
            self.status.set_vexpand(true);
            return;
        }

        self.container.append(&result_box);
        *self.current_child.borrow_mut() = Some(result_box.upcast::<Widget>());
        // Hide placeholder, let the scroll area fill the canvas
        self.status.set_visible(false);
        self.status.set_vexpand(false);
    }

    /// Clear the preview pane.
    pub fn clear(&self) {
        if let Some(old) = self.current_child.borrow_mut().take() {
            self.container.remove(&old);
        }
        self.status.set_text("Open a .ui file or create one via File → New .ui File");
        self.status.set_visible(true);
        self.status.set_vexpand(true);
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

    /// Register a callback invoked when the user double-clicks an interactive
    /// widget in the canvas.  The callback receives `(class, id)`.
    pub fn set_on_double_click<F: Fn(&str, &str) + 'static>(&self, cb: F) {
        *self.on_double_click.borrow_mut() = Some(Box::new(cb));
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
/// On double-click, fires the codegen callback for interactive widgets.
fn attach_click_to_select(widget: &Widget, byte_offset: usize, ctx: &ClickCtx, class: &str, id: &str) {
    let gesture = GestureClick::new();
    gesture.set_button(1); // left click
    // Use BUBBLE phase so DragSource (which uses CAPTURE internally) gets first chance
    gesture.set_propagation_phase(gtk4::PropagationPhase::Bubble);

    let buf_ref = ctx.buffer.clone();
    let xml_rc = ctx.xml.clone();
    let is_expander = class == "GtkExpander";
    let widget_ref = widget.clone();
    let selected_ref = ctx.selected_widget.clone();
    let dbl_click_cb = ctx.on_double_click.clone();
    let class_owned = class.to_string();
    let id_owned = id.to_string();

    // Use connect_released (not connect_pressed) so that DragSource gets a
    // chance to see motion events and claim the sequence for a drag before
    // GestureClick denies it.  In Bubble phase, connect_released fires deepest
    // widget first, so claiming here still prevents ancestor handlers from
    // triggering.
    gesture.connect_released(move |gesture, n_press, _x, _y| {
        // Claim the sequence — stops the event propagating to ancestor widgets.
        gesture.set_state(gtk4::EventSequenceState::Claimed);

        // ── Selection outline ──
        if let Some(prev) = selected_ref.borrow_mut().take() {
            prev.remove_css_class("canvas-selected");
        }
        widget_ref.add_css_class("canvas-selected");
        *selected_ref.borrow_mut() = Some(widget_ref.clone());

        // For Expander: toggle expand/collapse
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
        }

        // Double-click → fire codegen callback
        if n_press == 2 {
            if let Some(cb) = dbl_click_cb.borrow().as_ref() {
                cb(&class_owned, &id_owned);
            }
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

            // Collect child byte ranges and class names for reorder / insert-into
            let child_ranges: Rc<Vec<std::ops::Range<usize>>> = Rc::new(
                collect_child_ranges(node)
            );
            let sibling_classes: Rc<Vec<String>> = Rc::new(
                collect_sibling_classes(node)
            );
            for (i, (_ty, child)) in children.iter().enumerate() {
                // Right-click on child → context popover
                if i < child_ranges.len() {
                    let rc_gesture = GestureClick::new();
                    rc_gesture.set_button(3); // right-click
                    let idx = i;
                    let ranges = child_ranges.clone();
                    let classes = sibling_classes.clone();
                    let buf_ref = ctx.buffer.clone();
                    let xml_rc = ctx.xml.clone();
                    let child_for_popover = child.clone();

                    rc_gesture.connect_pressed(move |gesture, _n, _x, _y| {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);

                        // Build a small popover with Up / Down / ↵ Into buttons
                        let pop_box = GtkBox::new(Orientation::Horizontal, 4);
                        pop_box.set_margin_start(4);
                        pop_box.set_margin_end(4);
                        pop_box.set_margin_top(4);
                        pop_box.set_margin_bottom(4);

                        let up_btn = Button::with_label("▲ Up");
                        up_btn.add_css_class("flat");
                        up_btn.set_sensitive(idx > 0);

                        let down_btn = Button::with_label("▼ Down");
                        down_btn.add_css_class("flat");
                        down_btn.set_sensitive(idx + 1 < ranges.len());

                        // "↵ Into" — insert this child into an adjacent container
                        let into_btn = Button::with_label("↵ Into");
                        into_btn.add_css_class("flat");
                        // Sensitive if an adjacent sibling is a container
                        let has_container_neighbor = {
                            let above_is_container = idx > 0
                                && classes.get(idx - 1).map_or(false, |c| is_container_class(c));
                            let below_is_container = (idx + 1 < classes.len())
                                && classes.get(idx + 1).map_or(false, |c| is_container_class(c));
                            above_is_container || below_is_container
                        };
                        into_btn.set_sensitive(has_container_neighbor);

                        let del_btn = Button::with_label("✕ Del");
                        del_btn.add_css_class("flat");

                        pop_box.append(&up_btn);
                        pop_box.append(&down_btn);
                        pop_box.append(&into_btn);
                        pop_box.append(&del_btn);

                        let popover = Popover::new();
                        popover.set_child(Some(&pop_box));
                        popover.set_parent(&child_for_popover);
                        popover.set_autohide(true);

                        // Move Up
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = ranges.clone();
                            let from = idx;
                            let pop = popover.clone();
                            up_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if from > 0 {
                                    if let Some(b) = buf.borrow().as_ref() {
                                        reorder_xml_children(b, &xml, &r, from, from - 1);
                                    }
                                }
                            });
                        }

                        // Move Down
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = ranges.clone();
                            let from = idx;
                            let pop = popover.clone();
                            let len = ranges.len();
                            down_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if from + 1 < len {
                                    if let Some(b) = buf.borrow().as_ref() {
                                        reorder_xml_children(b, &xml, &r, from, from + 1);
                                    }
                                }
                            });
                        }

                        // Insert Into adjacent container
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = ranges.clone();
                            let cls = classes.clone();
                            let from = idx;
                            let pop = popover.clone();
                            into_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if let Some(b) = buf.borrow().as_ref() {
                                    insert_into_adjacent_container(b, &xml, &r, &cls, from);
                                }
                            });
                        }

                        // Delete
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = ranges.clone();
                            let from = idx;
                            let pop = popover.clone();
                            del_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if let Some(b) = buf.borrow().as_ref() {
                                    delete_child_range(b, &xml, &r[from]);
                                }
                            });
                        }

                        // Clean up popover when closed
                        {
                            let pop = popover.clone();
                            popover.connect_closed(move |_| {
                                pop.unparent();
                            });
                        }

                        popover.popup();
                    });
                    child.add_controller(rc_gesture);
                }

                // Drop target — reorder within this GtkBox
                {
                    let drop_tgt = DropTarget::new(
                        String::static_type(),
                        gtk4::gdk::DragAction::MOVE,
                    );
                    let dt_buf    = ctx.buffer.clone();
                    let dt_xml    = ctx.xml.clone();
                    let dt_ranges = child_ranges.clone();
                    let dt_to     = i;
                    drop_tgt.connect_drop(move |_, val, _x, _y| {
                        if let Ok(s) = val.get::<String>() {
                            if let Some((src_start, _)) = parse_child_range_payload(&s) {
                                if let Some(from) = dt_ranges.iter().position(|r| r.start == src_start) {
                                    if let Some(b) = dt_buf.borrow().as_ref() {
                                        reorder_xml_children(b, &dt_xml, &dt_ranges, from, dt_to);
                                    }
                                }
                            }
                        }
                        true
                    });
                    child.add_controller(drop_tgt);
                }

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
            if let Some(v) = prop_val(&props, "row-homogeneous") {
                g.set_row_homogeneous(parse_bool(v));
            }
            if let Some(v) = prop_val(&props, "column-homogeneous") {
                g.set_column_homogeneous(parse_bool(v));
            }
            apply_common_props(&g, &props);

            // Collect child nodes with their <layout> properties and byte ranges
            let child_nodes: Vec<_> = node
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "child")
                .filter_map(|child_node| {
                    let child_range = child_node.range();
                    let obj = child_node
                        .children()
                        .find(|n| n.is_element() && n.tag_name().name() == "object")?;
                    let widget = build_widget(obj, ctx)?;
                    let layout = collect_layout_props(obj);
                    Some((widget, layout, child_range))
                })
                .collect();

            if child_nodes.is_empty() {
                // No children — show placeholder grid so the user has somewhere
                // to drop widgets.  Use rui-rows/rui-columns if present.
                let n_rows = prop_val(&props, "rui-rows")
                    .or_else(|| prop_val(&props, "n-rows"))
                    .and_then(|v| v.trim().parse::<i32>().ok())
                    .unwrap_or(2);
                let n_cols = prop_val(&props, "rui-columns")
                    .or_else(|| prop_val(&props, "n-columns"))
                    .and_then(|v| v.trim().parse::<i32>().ok())
                    .unwrap_or(2);
                for r in 0..n_rows {
                    for c in 0..n_cols {
                        let lbl = Label::new(Some(&format!("{},{}", r, c)));
                        lbl.add_css_class("dim-label");
                        lbl.set_hexpand(true);
                        lbl.set_vexpand(true);
                        g.attach(&lbl, c, r, 1, 1);
                    }
                }
            } else {
                // Attach children using layout properties or auto-stack
                let mut auto_row = 0i32;
                let mut occupied = std::collections::HashSet::<(i32, i32)>::new();
                for (child, layout, child_range) in &child_nodes {
                    let col = layout.column.unwrap_or(0);
                    let row = layout.row.unwrap_or(auto_row);
                    let col_span = layout.column_span.unwrap_or(1);
                    let row_span = layout.row_span.unwrap_or(1);
                    g.attach(child, col, row, col_span, row_span);
                    auto_row = row + row_span;

                    // Right-click → grid layout editor popover
                    let rc_gesture = GestureClick::new();
                    rc_gesture.set_button(3);
                    let buf_ref = ctx.buffer.clone();
                    let xml_rc = ctx.xml.clone();
                    let child_for_pop = child.clone();
                    let init_col = col;
                    let init_row = row;
                    let init_cs = col_span;
                    let init_rs = row_span;
                    let range = child_range.clone();

                    rc_gesture.connect_pressed(move |gesture, _, _, _| {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);

                        let pop_vbox = GtkBox::new(Orientation::Vertical, 4);
                        pop_vbox.set_margin_start(8);
                        pop_vbox.set_margin_end(8);
                        pop_vbox.set_margin_top(6);
                        pop_vbox.set_margin_bottom(6);

                        // Helper: labelled SpinButton row
                        let make_spin = |label_text: &str, init: i32| -> (GtkBox, SpinButton) {
                            let row_box = GtkBox::new(Orientation::Horizontal, 8);
                            let lbl = Label::new(Some(label_text));
                            lbl.set_width_chars(10);
                            lbl.set_halign(gtk4::Align::Start);
                            let adj = Adjustment::new(init as f64, 0.0, 99.0, 1.0, 5.0, 0.0);
                            let spin = SpinButton::new(Some(&adj), 1.0, 0);
                            spin.set_width_chars(4);
                            row_box.append(&lbl);
                            row_box.append(&spin);
                            (row_box, spin)
                        };

                        let (col_row_w, col_spin) = make_spin("Column:", init_col);
                        let (row_row_w, row_spin) = make_spin("Row:", init_row);
                        let (cs_row_w, cs_spin) = make_spin("Col span:", init_cs);
                        let (rs_row_w, rs_spin) = make_spin("Row span:", init_rs);

                        pop_vbox.append(&col_row_w);
                        pop_vbox.append(&row_row_w);
                        pop_vbox.append(&cs_row_w);
                        pop_vbox.append(&rs_row_w);

                        let btn_box = GtkBox::new(Orientation::Horizontal, 4);
                        btn_box.set_margin_top(4);
                        let apply_btn = Button::with_label("Apply");
                        apply_btn.add_css_class("suggested-action");
                        let gdel_btn = Button::with_label("✕ Del");
                        gdel_btn.add_css_class("destructive-action");
                        btn_box.append(&apply_btn);
                        btn_box.append(&gdel_btn);
                        pop_vbox.append(&btn_box);

                        let popover = Popover::new();
                        popover.set_child(Some(&pop_vbox));
                        popover.set_parent(&child_for_pop);
                        popover.set_autohide(true);

                        // Apply layout changes
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = range.clone();
                            let col_s = col_spin.clone();
                            let row_s = row_spin.clone();
                            let cs_s = cs_spin.clone();
                            let rs_s = rs_spin.clone();
                            let pop = popover.clone();
                            apply_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if let Some(b) = buf.borrow().as_ref() {
                                    update_grid_child_layout(
                                        b, &xml, &r,
                                        col_s.value() as i32,
                                        row_s.value() as i32,
                                        cs_s.value() as i32,
                                        rs_s.value() as i32,
                                    );
                                }
                            });
                        }

                        // Delete
                        {
                            let buf = buf_ref.clone();
                            let xml = xml_rc.clone();
                            let r = range.clone();
                            let pop = popover.clone();
                            gdel_btn.connect_clicked(move |_| {
                                pop.popdown();
                                if let Some(b) = buf.borrow().as_ref() {
                                    delete_child_range(b, &xml, &r);
                                }
                            });
                        }

                        {
                            let pop = popover.clone();
                            popover.connect_closed(move |_| { pop.unparent(); });
                        }

                        popover.popup();
                    });
                    child.add_controller(rc_gesture);

                    // Mark every cell covered by this widget
                    for dr in 0..row_span {
                        for dc in 0..col_span {
                            occupied.insert((col + dc, row + dr));
                        }
                    }

                    // Drop target on occupied cell — move dragged widget here
                    {
                        let drop_tgt = DropTarget::new(
                            String::static_type(),
                            gtk4::gdk::DragAction::MOVE,
                        );
                        let dt_buf = ctx.buffer.clone();
                        let dt_xml = ctx.xml.clone();
                        let dt_col = col;
                        let dt_row = row;
                        let dt_cs  = col_span;
                        let dt_rs  = row_span;
                        drop_tgt.connect_drop(move |_, val, _x, _y| {
                            if let Ok(s) = val.get::<String>() {
                                if let Some((src_start, src_end)) = parse_child_range_payload(&s) {
                                    let src_range = src_start..src_end;
                                    if let Some(b) = dt_buf.borrow().as_ref() {
                                        update_grid_child_layout(b, &dt_xml, &src_range, dt_col, dt_row, dt_cs, dt_rs);
                                    }
                                }
                            }
                            true
                        });
                        child.add_controller(drop_tgt);
                    }
                }

                // Add empty-cell placeholders for all grid positions not yet occupied.
                // These act as drop targets so the user can drag widgets into empty cells.
                // rui-rows / rui-columns are custom props set by make_grid_template so
                // we know the intended dimensions even when no widgets are placed yet.
                let rui_cols = prop_val(&props, "rui-columns")
                    .and_then(|v| v.trim().parse::<i32>().ok());
                let rui_rows = prop_val(&props, "rui-rows")
                    .and_then(|v| v.trim().parse::<i32>().ok());

                let content_max_col = child_nodes.iter()
                    .map(|(_, l, _)| l.column.unwrap_or(0) + l.column_span.unwrap_or(1))
                    .max().unwrap_or(0);
                let content_max_row = child_nodes.iter()
                    .map(|(_, l, _)| l.row.unwrap_or(0) + l.row_span.unwrap_or(1))
                    .max().unwrap_or(0);

                // Use rui- dimensions as the floor; content may exceed them
                let max_col   = rui_cols.unwrap_or(content_max_col).max(content_max_col);
                let max_row_g = rui_rows.unwrap_or(content_max_row).max(content_max_row);

                for gr in 0..max_row_g {
                    for gc in 0..max_col {
                        if !occupied.contains(&(gc, gr)) {
                            let ph = Frame::new(None);
                            ph.set_hexpand(true);
                            ph.set_vexpand(true);
                            ph.set_size_request(60, 36);
                            let coord_lbl = Label::new(Some(&format!("({},{})", gc, gr)));
                            coord_lbl.add_css_class("dim-label");
                            ph.set_child(Some(&coord_lbl));

                            let drop_tgt = DropTarget::new(
                                String::static_type(),
                                gtk4::gdk::DragAction::MOVE,
                            );
                            let dt_buf = ctx.buffer.clone();
                            let dt_xml = ctx.xml.clone();
                            let dt_c = gc;
                            let dt_r = gr;
                            drop_tgt.connect_drop(move |_, val, _x, _y| {
                                if let Ok(s) = val.get::<String>() {
                                    if let Some((src_start, src_end)) = parse_child_range_payload(&s) {
                                        let src_range = src_start..src_end;
                                        if let Some(b) = dt_buf.borrow().as_ref() {
                                            update_grid_child_layout(b, &dt_xml, &src_range, dt_c, dt_r, 1, 1);
                                        }
                                    }
                                }
                                true
                            });
                            ph.add_controller(drop_tgt);

                            g.attach(&ph, gc, gr, 1, 1);
                        }
                    }
                }
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
    let id = node.attribute("id").unwrap_or("");
    attach_click_to_select(&widget, byte_offset, ctx, class, id);

    // Drag source — only for objects that are a direct <child> of a container.
    // Payload: "{child_start}:{child_end}" byte range of the <child> block.
    if let Some(parent_child) = node.parent()
        .filter(|p| p.is_element() && p.tag_name().name() == "child")
    {
        let cr = parent_child.range();
        let payload = format!("{}:{}", cr.start, cr.end);
        let drag_src = DragSource::new();
        drag_src.set_actions(gtk4::gdk::DragAction::MOVE);
        // Capture phase so DragSource sees press/motion before the widget's
        // own internal gesture handlers (GtkButton, GtkEntry, etc.) which
        // live in Bubble phase — without this, interactive widgets can't be
        // dragged because their built-in gestures claim the sequence first.
        drag_src.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let content = gtk4::gdk::ContentProvider::for_value(&payload.to_value());
        drag_src.set_content(Some(&content));
        widget.add_controller(drag_src);
    }

    Some(widget)
}

// ─────────────────────────────────────────────────────────────────────
// Layout parsing for Grid children
// ─────────────────────────────────────────────────────────────────────

/// Layout properties from a `<layout>` child of an `<object>`.
struct LayoutProps {
    column: Option<i32>,
    row: Option<i32>,
    column_span: Option<i32>,
    row_span: Option<i32>,
}

/// Parse `<layout><property name="column">...</property>...` inside an <object>.
fn collect_layout_props(obj_node: roxmltree::Node) -> LayoutProps {
    let layout_node = obj_node
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "layout");

    let mut lp = LayoutProps {
        column: None,
        row: None,
        column_span: None,
        row_span: None,
    };

    if let Some(layout) = layout_node {
        for prop in layout
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "property")
        {
            let name = prop.attribute("name").unwrap_or("");
            let val = prop.text().unwrap_or("").trim();
            match name {
                "column" => lp.column = val.parse().ok(),
                "row" => lp.row = val.parse().ok(),
                "column-span" => lp.column_span = val.parse().ok(),
                "row-span" => lp.row_span = val.parse().ok(),
                _ => {}
            }
        }
    }

    lp
}

// ─────────────────────────────────────────────────────────────────────
// Container detection helpers
// ─────────────────────────────────────────────────────────────────────

/// Check if a GTK class name is a container widget.
fn is_container_class(class: &str) -> bool {
    matches!(
        class,
        "GtkBox"
            | "GtkGrid"
            | "GtkFrame"
            | "GtkScrolledWindow"
            | "GtkPaned"
            | "GtkNotebook"
            | "GtkStack"
            | "GtkOverlay"
            | "GtkCenterBox"
            | "GtkExpander"
            | "GtkFlowBox"
            | "GtkListBox"
            | "GtkHeaderBar"
            | "GtkActionBar"
            | "GtkWindow"
            | "GtkApplicationWindow"
            | "GtkDialog"
            | "GtkPopover"
            | "GtkRevealer"
            | "GtkViewport"
    )
}

/// Collect the class names of each `<child>`'s `<object>` under a parent node.
/// Result indices align with `collect_child_ranges`.
fn collect_sibling_classes(node: roxmltree::Node) -> Vec<String> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "child")
        .filter_map(|child_node| {
            let obj = child_node
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "object")?;
            Some(obj.attribute("class").unwrap_or("").to_string())
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────
// Drag-to-reorder helpers
// ─────────────────────────────────────────────────────────────────────

/// Collect byte ranges of all `<child>` elements under a node.
/// Indices align with `collect_children` for typical container widgets
/// (children that have a buildable `<object>` child).
fn collect_child_ranges(node: roxmltree::Node) -> Vec<std::ops::Range<usize>> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "child")
        .filter_map(|child_node| {
            // Only include children that have an <object> (matching collect_children filter)
            child_node
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "object")?;
            Some(child_node.range())
        })
        .collect()
}

/// Reorder `<child>` blocks in the source buffer by moving the child
/// at index `from` to index `to`.
fn reorder_xml_children(
    buffer: &sourceview5::Buffer,
    xml: &str,
    ranges: &[std::ops::Range<usize>],
    from: usize,
    to: usize,
) {
    if from == to || ranges.len() < 2 || from >= ranges.len() || to >= ranges.len() {
        return;
    }

    // Collect child block texts in original order
    let mut blocks: Vec<String> = ranges.iter()
        .map(|r| xml[r.clone()].to_string())
        .collect();

    // Separator between children (whitespace/newlines from the original)
    let separator = if ranges.len() >= 2 {
        xml[ranges[0].end..ranges[1].start].to_string()
    } else {
        "\n    ".to_string()
    };

    // Move block from source to target position
    let block = blocks.remove(from);
    blocks.insert(to, block);

    // Reconstruct the children region
    let new_text = blocks.join(&separator);

    // Replace the entire span from first child start to last child end
    let overall_start = ranges.iter().map(|r| r.start).min().unwrap();
    let overall_end = ranges.iter().map(|r| r.end).max().unwrap();

    // Convert byte offsets to char offsets
    let char_start = xml[..overall_start].chars().count();
    let char_end = xml[..overall_end].chars().count();

    let mut start_iter = buffer.iter_at_offset(char_start as i32);
    let mut end_iter = buffer.iter_at_offset(char_end as i32);

    buffer.begin_user_action();
    buffer.delete(&mut start_iter, &mut end_iter);
    buffer.insert(&mut start_iter, &new_text);
    buffer.end_user_action();
}

/// Move the `<child>` block at index `from` inside the nearest adjacent
/// container sibling. Prefers the sibling above; falls back to below.
///
/// The moved `<child>` block is appended just before the container's
/// closing `</object>` tag, preserving indentation.
fn insert_into_adjacent_container(
    buffer: &sourceview5::Buffer,
    xml: &str,
    ranges: &[std::ops::Range<usize>],
    classes: &[String],
    from: usize,
) {
    if from >= ranges.len() || from >= classes.len() {
        return;
    }

    // Determine which adjacent sibling is a container (prefer above)
    let target_idx = if from > 0 && classes.get(from - 1).map_or(false, |c| is_container_class(c)) {
        from - 1
    } else if from + 1 < classes.len()
        && classes.get(from + 1).map_or(false, |c| is_container_class(c))
    {
        from + 1
    } else {
        return;
    };

    // Extract the child block to move
    let child_block = xml[ranges[from].clone()].to_string();

    // Find the container's </object> closing tag.
    // The container is inside its own <child>…</child> wrapper.
    // We need to find the *last* </object> inside that <child> range.
    let container_range = &ranges[target_idx];
    let container_text = &xml[container_range.clone()];

    // Find the last </object> in the container (this is the container object's close)
    let last_close = match container_text.rfind("</object>") {
        Some(pos) => pos,
        None => return,
    };

    // Determine the indentation for the new child block.
    // Look at the whitespace before </object> to get the container's indent level.
    let before_close = &container_text[..last_close];
    let indent = before_close
        .rfind('\n')
        .map(|nl| {
            let line_start = nl + 1;
            let spaces = &before_close[line_start..];
            spaces
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        })
        .unwrap_or_else(|| "    ".to_string());

    // Re-indent the child block to match the container's depth (one level deeper)
    let child_indent = format!("{}  ", indent);
    let indented_child = reindent_block(&child_block, &child_indent);

    // Build the new text to insert: newline + indented child block + newline + indent
    let insertion = format!("\n{}{}\n{}", child_indent, indented_child.trim(), indent);

    // Absolute byte offset of the insertion point (before the container's </object>)
    let insert_byte = container_range.start + last_close;

    // Now we need to:
    // 1. Remove the original child block (and surrounding whitespace)
    // 2. Insert it inside the container
    // We must be careful about ordering: removing vs inserting changes offsets.

    // Strategy: build the new full text, then replace the entire affected region.
    let mut new_xml = String::with_capacity(xml.len() + insertion.len());

    if from < target_idx {
        // Child is above the container: remove child first, then insert
        // Remove the child block and trailing whitespace up to next tag
        let remove_start = ranges[from].start;
        let remove_end = if from + 1 < ranges.len() {
            // Eat whitespace between this child and the next
            ranges[from + 1].start
        } else {
            ranges[from].end
        };

        new_xml.push_str(&xml[..remove_start]);
        let after_remove = &xml[remove_end..];

        // The insert point shifted by the removal amount
        let shifted_insert = insert_byte - (remove_end - remove_start);
        let remaining_before_insert = shifted_insert - remove_start;
        new_xml.push_str(&after_remove[..remaining_before_insert]);
        new_xml.push_str(&insertion);
        new_xml.push_str(&after_remove[remaining_before_insert..]);
    } else {
        // Child is below the container: insert first, then remove
        new_xml.push_str(&xml[..insert_byte]);
        new_xml.push_str(&insertion);
        new_xml.push_str(&xml[insert_byte..ranges[from].start]);
        // Skip the child block and trailing whitespace
        let remove_end = if from + 1 < ranges.len() {
            ranges[from + 1].start
        } else {
            ranges[from].end
        };
        new_xml.push_str(&xml[remove_end..]);
    }

    // Replace the entire buffer
    let (buf_start, buf_end) = buffer.bounds();
    let mut s = buf_start;
    let mut e = buf_end;

    buffer.begin_user_action();
    buffer.delete(&mut s, &mut e);
    buffer.insert(&mut s, &new_xml);
    buffer.end_user_action();
}

/// Re-indent a block of text to a given base indentation.
fn reindent_block(block: &str, base_indent: &str) -> String {
    let lines: Vec<&str> = block.lines().collect();
    if lines.is_empty() {
        return block.to_string();
    }

    // Find the minimum indentation of non-empty lines
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{}{}", base_indent, &line[min_indent..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─────────────────────────────────────────────────────────────────────
// Container edit helpers (delete child, update grid layout)
// ─────────────────────────────────────────────────────────────────────

/// Find the byte length of a complete `<object>...</object>` block starting at `s[0]`.
fn find_obj_close(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut pos = 0;
    while pos < s.len() {
        if s[pos..].starts_with("<object") {
            depth += 1;
            pos += 7;
        } else if s[pos..].starts_with("</object>") {
            depth -= 1;
            if depth == 0 {
                return Some(pos + 9);
            }
            pos += 9;
        } else {
            let ch_len = s[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            pos += ch_len;
        }
    }
    None
}

/// Delete a `<child>…</child>` block from the source buffer.
/// Also removes the preceding indentation whitespace and the trailing newline.
fn delete_child_range(buffer: &sourceview5::Buffer, xml: &str, range: &std::ops::Range<usize>) {
    if range.start > xml.len() || range.end > xml.len() {
        return;
    }
    // Walk back to start of line
    let remove_start = xml[..range.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Eat trailing newline
    let remove_end = if range.end < xml.len() && xml.as_bytes()[range.end] == b'\n' {
        range.end + 1
    } else {
        range.end
    };
    let char_start = xml[..remove_start].chars().count();
    let char_end   = xml[..remove_end].chars().count();
    let mut si = buffer.iter_at_offset(char_start as i32);
    let mut ei = buffer.iter_at_offset(char_end as i32);
    buffer.begin_user_action();
    buffer.delete(&mut si, &mut ei);
    buffer.end_user_action();
}

/// Replace (or create) the `<layout>` block inside a GtkGrid child's `<object>`
/// with fresh column/row/column-span/row-span values.
fn update_grid_child_layout(
    buffer: &sourceview5::Buffer,
    xml: &str,
    child_range: &std::ops::Range<usize>,
    col: i32,
    row: i32,
    col_span: i32,
    row_span: i32,
) {
    if child_range.end > xml.len() {
        return;
    }
    let child_block = &xml[child_range.clone()];

    // Find the <object> inside this <child>
    let obj_rel = match child_block.find("<object") {
        Some(i) => i,
        None => return,
    };
    let obj_abs_start = child_range.start + obj_rel;
    let obj_text = &xml[obj_abs_start..child_range.end];

    // Find the length of the <object>...</object> block (handles nesting)
    let obj_len = match find_obj_close(obj_text) {
        Some(e) => e,
        None => return,
    };
    let obj_block = &xml[obj_abs_start..obj_abs_start + obj_len];

    // Determine indentation of the <object> tag line
    let obj_line_start = xml[..obj_abs_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let obj_indent: String = xml[obj_line_start..obj_abs_start]
        .chars().take_while(|c| c.is_whitespace()).collect();
    let layout_indent = format!("{}  ", obj_indent);
    let prop_indent   = format!("{}    ", obj_indent);

    let new_layout = format!(
        "<layout>\n\
{pi}<property name=\"column\">{col}</property>\n\
{pi}<property name=\"row\">{row}</property>\n\
{pi}<property name=\"column-span\">{cs}</property>\n\
{pi}<property name=\"row-span\">{rs}</property>\n\
{li}</layout>",
        pi = prop_indent, li = layout_indent,
        col = col, row = row, cs = col_span, rs = row_span
    );

    // Try to replace an existing <layout>...</layout>
    if let Some(layout_rel) = obj_block.find("<layout>") {
        if let Some(close_rel) = obj_block[layout_rel..].find("</layout>") {
            let layout_abs_start = obj_abs_start + layout_rel;
            let layout_abs_end   = obj_abs_start + layout_rel + close_rel + "</layout>".len();
            let char_start = xml[..layout_abs_start].chars().count();
            let char_end   = xml[..layout_abs_end].chars().count();
            let mut si = buffer.iter_at_offset(char_start as i32);
            let mut ei = buffer.iter_at_offset(char_end as i32);
            buffer.begin_user_action();
            buffer.delete(&mut si, &mut ei);
            buffer.insert(&mut si, &new_layout);
            buffer.end_user_action();
            return;
        }
    }

    // No <layout> block — insert one before </object>
    let close_obj_rel = match obj_block.rfind("</object>") {
        Some(i) => i,
        None => return,
    };
    let insert_byte = obj_abs_start + close_obj_rel;
    let insertion = format!("\n{}{}", layout_indent, new_layout);
    let char_pos = xml[..insert_byte].chars().count();
    let mut iter = buffer.iter_at_offset(char_pos as i32);
    buffer.begin_user_action();
    buffer.insert(&mut iter, &insertion);
    buffer.end_user_action();
}

/// Parse a drag-and-drop payload of the form `"{start}:{end}"` into a byte range pair.
fn parse_child_range_payload(s: &str) -> Option<(usize, usize)> {
    let mut parts = s.splitn(2, ':');
    let start = parts.next()?.parse::<usize>().ok()?;
    let end   = parts.next()?.parse::<usize>().ok()?;
    Some((start, end))
}
