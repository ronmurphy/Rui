//! node_view.rs — Widget relationship node graph.
//!
//! Third centre-pane tab (alongside Design and Code).
//! Displays the widget hierarchy from the active .ui XML as an interactive
//! node graph, plus signal-handler edges parsed heuristically from the
//! companion .rs file.
//!
//! • Click a widget node  → moves the XML-editor cursor to that <object…>.
//! • Click a handler node → moves the cursor to the connect_* line in the .rs.
//! • Drag to pan, scroll to zoom, "Fit" button to re-centre.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DrawingArea, EventControllerScroll,
    GestureClick, GestureDrag, Label, Orientation,
};
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sourceview5::prelude::*;

// ── Layout constants ──────────────────────────────────────────────────────────

const NODE_W:      f64 = 172.0;
const NODE_H:      f64 = 54.0;
const LEVEL_GAP:   f64 = 86.0;
const SIBLING_GAP: f64 = 28.0;
const DEBOUNCE_MS: u64 = 500;

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct WidgetNode {
    id:         String,
    class_name: String,   // "Button", "Label", … (Gtk prefix stripped)
    parent:     Option<usize>,
    children:   Vec<usize>,
    x:          f64,
    y:          f64,
    xml_offset: usize,    // character offset into the XML buffer
}

#[derive(Clone, Default)]
struct HandlerNode {
    widget_id: String,
    signal:    String,   // "clicked", "changed", …
    rs_line:   usize,    // 1-based line number in the .rs file
    x:         f64,
    y:         f64,
}

#[derive(Clone)]
struct SignalEdge {
    widget_idx:  usize,
    handler_idx: usize,
}

// ── Inner shared state ────────────────────────────────────────────────────────

struct Inner {
    area:        DrawingArea,
    info_lbl:    Label,
    nodes:       RefCell<Vec<WidgetNode>>,
    handlers:    RefCell<Vec<HandlerNode>>,
    edges:       RefCell<Vec<SignalEdge>>,
    pan:         Cell<(f64, f64)>,
    scale:       Cell<f64>,
    pan_at_drag: Cell<(f64, f64)>,
    selected:    Cell<Option<usize>>,
    generation:  Cell<u64>,
    on_jump_xml: RefCell<Option<Box<dyn Fn(usize)>>>,  // char offset
    on_jump_rs:  RefCell<Option<Box<dyn Fn(usize)>>>,  // line number (1-based)
}

// ── Public struct ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct NodeView {
    pub widget: GtkBox,
    inner:      Rc<Inner>,
}

impl NodeView {
    pub fn new() -> Self {
        let outer = GtkBox::new(Orientation::Vertical, 0);

        // ── Top bar ───────────────────────────────────────────────────────────
        let top_bar = GtkBox::new(Orientation::Horizontal, 6);
        top_bar.set_margin_start(8);
        top_bar.set_margin_end(8);
        top_bar.set_margin_top(4);
        top_bar.set_margin_bottom(4);

        let fit_btn = Button::with_label("Fit");
        fit_btn.set_tooltip_text(Some("Reset zoom and centre the graph (F)"));
        fit_btn.add_css_class("flat");

        let info_lbl = Label::new(Some("Open a .ui file to see its widget graph"));
        info_lbl.set_halign(gtk4::Align::Start);
        info_lbl.set_hexpand(true);
        info_lbl.add_css_class("dim-label");

        top_bar.append(&fit_btn);
        top_bar.append(&info_lbl);

        // ── Drawing area ──────────────────────────────────────────────────────
        let area = DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_focusable(true);

        outer.append(&top_bar);
        outer.append(&area);

        let inner = Rc::new(Inner {
            area:        area.clone(),
            info_lbl:    info_lbl.clone(),
            nodes:       RefCell::new(Vec::new()),
            handlers:    RefCell::new(Vec::new()),
            edges:       RefCell::new(Vec::new()),
            pan:         Cell::new((0.0, 0.0)),
            scale:       Cell::new(1.0),
            pan_at_drag: Cell::new((0.0, 0.0)),
            selected:    Cell::new(None),
            generation:  Cell::new(0),
            on_jump_xml: RefCell::new(None),
            on_jump_rs:  RefCell::new(None),
        });

        // ── Draw function ─────────────────────────────────────────────────────
        {
            let i = inner.clone();
            area.set_draw_func(move |a, cr, _, _| {
                draw_graph(&i, cr, a.allocated_width() as f64, a.allocated_height() as f64);
            });
        }

        // ── Fit button ────────────────────────────────────────────────────────
        {
            let i = inner.clone();
            fit_btn.connect_clicked(move |_| {
                fit_to_view(&i);
                i.area.queue_draw();
            });
        }

        // ── Pan (drag) ────────────────────────────────────────────────────────
        {
            let drag = GestureDrag::new();
            let i = inner.clone();
            drag.connect_drag_begin(move |_, _, _| {
                i.pan_at_drag.set(i.pan.get());
            });
            let i = inner.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                let (ox, oy) = i.pan_at_drag.get();
                i.pan.set((ox + dx, oy + dy));
                i.area.queue_draw();
            });
            area.add_controller(drag);
        }

        // ── Zoom (scroll) ─────────────────────────────────────────────────────
        {
            let scroll = EventControllerScroll::new(
                gtk4::EventControllerScrollFlags::VERTICAL,
            );
            let i = inner.clone();
            scroll.connect_scroll(move |_, _dx, dy| {
                let s = i.scale.get();
                let ns = (s * if dy < 0.0 { 1.1 } else { 0.9 }).clamp(0.15, 4.0);
                i.scale.set(ns);
                i.area.queue_draw();
                glib::Propagation::Stop
            });
            area.add_controller(scroll);
        }

        // ── Click to select / navigate ────────────────────────────────────────
        {
            let click = GestureClick::new();
            click.set_button(1);
            let i = inner.clone();
            click.connect_released(move |_, _, x, y| {
                let hit = hit_test(&i, x, y);
                i.selected.set(hit);
                i.area.queue_draw();

                if let Some(idx) = hit {
                    let nodes   = i.nodes.borrow();
                    let handlers = i.handlers.borrow();
                    let nlen = nodes.len();
                    if idx < nlen {
                        if let Some(cb) = i.on_jump_xml.borrow().as_ref() {
                            cb(nodes[idx].xml_offset);
                        }
                    } else {
                        let hi = idx - nlen;
                        if let Some(h) = handlers.get(hi) {
                            if h.rs_line > 0 {
                                if let Some(cb) = i.on_jump_rs.borrow().as_ref() {
                                    cb(h.rs_line);
                                }
                            }
                        }
                    }
                }
            });
            area.add_controller(click);
        }

        // ── 'F' key → fit ─────────────────────────────────────────────────────
        {
            let key = gtk4::EventControllerKey::new();
            let i = inner.clone();
            key.connect_key_pressed(move |_, key, _, _| {
                if key == gtk4::gdk::Key::f || key == gtk4::gdk::Key::F {
                    fit_to_view(&i);
                    i.area.queue_draw();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            area.add_controller(key);
        }

        Self { widget: outer, inner }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Callback fired when a widget node is clicked — receives the character
    /// offset in the .ui XML buffer to place the cursor at.
    pub fn on_jump_xml<F: Fn(usize) + 'static>(&self, cb: F) {
        *self.inner.on_jump_xml.borrow_mut() = Some(Box::new(cb));
    }

    /// Callback fired when a handler node is clicked — receives the 1-based
    /// line number in the companion .rs file.
    pub fn on_jump_rs<F: Fn(usize) + 'static>(&self, cb: F) {
        *self.inner.on_jump_rs.borrow_mut() = Some(Box::new(cb));
    }

    /// Connect to a .ui source buffer.  Parses immediately and re-parses on
    /// every subsequent buffer change (debounced to 500 ms).
    pub fn connect_buffer(&self, buffer: &sourceview5::Buffer) {
        let (s, e) = buffer.bounds();
        let xml = buffer.text(&s, &e, false).to_string();
        self.inner.set_widget_nodes(&xml);
        self.relayout_and_fit();

        let inner = self.inner.clone();
        buffer.connect_changed(move |buf| {
            let gen = inner.generation.get().wrapping_add(1);
            inner.generation.set(gen);
            let inner2 = inner.clone();
            let buf2 = buf.clone();
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(DEBOUNCE_MS),
                move || {
                    if inner2.generation.get() == gen {
                        let (s, e) = buf2.bounds();
                        let xml = buf2.text(&s, &e, false).to_string();
                        inner2.set_widget_nodes(&xml);
                        inner2.rebuild_edges();
                        inner2.area.queue_draw();
                    }
                },
            );
        });
    }

    /// Supply the raw content of the companion .rs file so signal edges can
    /// be drawn.  Call this every time a .ui file is (re-)opened.
    pub fn set_rs_content(&self, rs: &str) {
        let handlers = parse_rs_handlers(rs);
        *self.inner.handlers.borrow_mut() = handlers;
        self.inner.rebuild_edges();
        self.relayout_and_fit();
    }

    /// Clear the graph (called when a non-.ui file becomes active).
    pub fn clear(&self) {
        self.inner.nodes.borrow_mut().clear();
        self.inner.handlers.borrow_mut().clear();
        self.inner.edges.borrow_mut().clear();
        self.inner.selected.set(None);
        self.inner.info_lbl.set_text("Open a .ui file to see its widget graph");
        self.inner.area.queue_draw();
    }

    /// Re-queue a draw (e.g. when the Nodes tab becomes visible).
    pub fn refresh(&self) {
        self.inner.area.queue_draw();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn relayout_and_fit(&self) {
        {
            let mut nodes    = self.inner.nodes.borrow_mut();
            let mut handlers = self.inner.handlers.borrow_mut();
            layout_nodes(&mut nodes, &mut handlers);
        }
        let n = self.inner.nodes.borrow().len();
        let h = self.inner.handlers.borrow().len();
        let txt = if n == 0 {
            "No widgets found".to_string()
        } else {
            format!("{} widget{},  {} signal handler{}",
                n, if n == 1 { "" } else { "s" },
                h, if h == 1 { "" } else { "s" })
        };
        self.inner.info_lbl.set_text(&txt);
        fit_to_view(&self.inner);
        self.inner.area.queue_draw();
    }
}

// ── Inner methods ─────────────────────────────────────────────────────────────

impl Inner {
    fn set_widget_nodes(&self, xml: &str) {
        *self.nodes.borrow_mut() = parse_xml_nodes(xml);
    }

    fn rebuild_edges(&self) {
        let nodes    = self.nodes.borrow();
        let handlers = self.handlers.borrow();
        *self.edges.borrow_mut() = build_signal_edges(&nodes, &handlers);
    }
}

// ── XML parsing ───────────────────────────────────────────────────────────────

fn parse_xml_nodes(xml: &str) -> Vec<WidgetNode> {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d)  => d,
        Err(_) => return vec![],
    };
    let mut nodes: Vec<WidgetNode> = Vec::new();
    walk_node(doc.root_element(), None, xml, &mut nodes);
    nodes
}

fn walk_node<'i, 'a>(
    node:       roxmltree::Node<'i, 'a>,
    parent_idx: Option<usize>,
    xml:        &str,
    nodes:      &mut Vec<WidgetNode>,
) {
    if node.tag_name().name() == "object" {
        let class = node.attribute("class").unwrap_or("Unknown");
        let id    = node.attribute("id").unwrap_or("").to_string();
        let class_name = class.strip_prefix("Gtk").unwrap_or(class).to_string();

        // byte → char offset so TextBuffer::iter_at_offset works correctly
        let byte_start = node.range().start;
        let xml_offset = xml[..byte_start.min(xml.len())].chars().count();

        let idx = nodes.len();
        nodes.push(WidgetNode {
            id, class_name, parent: parent_idx,
            children: vec![], x: 0.0, y: 0.0, xml_offset,
        });
        if let Some(pi) = parent_idx {
            nodes[pi].children.push(idx);
        }
        for child in node.children() {
            walk_node(child, Some(idx), xml, nodes);
        }
    } else {
        for child in node.children() {
            walk_node(child, parent_idx, xml, nodes);
        }
    }
}

// ── .rs signal parsing ────────────────────────────────────────────────────────

const SIGNALS: &[&str] = &[
    "connect_clicked", "connect_toggled", "connect_changed",
    "connect_released", "connect_pressed", "connect_activated",
    "connect_value_changed", "connect_row_activated",
    "connect_notify", "connect_text_changed",
];

fn parse_rs_handlers(rs: &str) -> Vec<HandlerNode> {
    let mut var_to_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Pass 1: collect builder.object("id") → variable name mappings
    for line in rs.lines() {
        if let (Some(id), Some(var)) = (extract_object_id(line), extract_let_var(line)) {
            var_to_id.insert(var, id);
        }
    }

    // Pass 2: find connect_* calls and link back to widget ids
    let mut handlers = Vec::new();
    for (line_idx, line) in rs.lines().enumerate() {
        for &sig in SIGNALS {
            if !line.contains(sig) { continue; }
            if let Some(var) = extract_receiver(line, sig) {
                if let Some(widget_id) = var_to_id.get(&var) {
                    let signal = sig.strip_prefix("connect_").unwrap_or(sig).to_string();
                    handlers.push(HandlerNode {
                        widget_id: widget_id.clone(),
                        signal,
                        rs_line: line_idx + 1,
                        x: 0.0, y: 0.0,
                    });
                }
            }
        }
    }
    handlers
}

/// Extract "widget_id" from `.object("widget_id")` or `.object::<T>("widget_id")`.
fn extract_object_id(line: &str) -> Option<String> {
    let pos = line.find(".object")?;
    // Find the opening paren (handles both .object( and .object::<T>()
    let rel_paren = line[pos..].find('(')?;
    let after_paren = &line[pos + rel_paren + 1..];
    // Find the first quote = start of the id argument
    let q1 = after_paren.find('"')?;
    let id_body = &after_paren[q1 + 1..];
    let q2 = id_body.find('"')?;
    let id = &id_body[..q2];
    if id.is_empty() { return None; }
    Some(id.to_string())
}

/// Extract variable name from `let VAR: … =` or `let mut VAR: … =`.
fn extract_let_var(line: &str) -> Option<String> {
    let t = line.trim();
    let t = t.strip_prefix("let mut ").or_else(|| t.strip_prefix("let "))?;
    let var = t.split_whitespace().next()?
               .split(':').next()?
               .to_string();
    if var.is_empty() || var == "_" { None } else { Some(var) }
}

/// Extract receiver variable from `receiver.connect_signal(`.
fn extract_receiver(line: &str, connect_fn: &str) -> Option<String> {
    let pos   = line.find(connect_fn)?;
    let before = line[..pos].trim_end();
    let dot   = before.rfind('.')?;
    let var   = before[..dot].trim();
    let var   = var.split_whitespace().last()?;
    let var   = var.trim_start_matches('&').trim_start_matches('*');
    if var.is_empty() { None } else { Some(var.to_string()) }
}

// ── Signal edge builder ───────────────────────────────────────────────────────

fn build_signal_edges(nodes: &[WidgetNode], handlers: &[HandlerNode]) -> Vec<SignalEdge> {
    let mut edges = Vec::new();
    for (hi, h) in handlers.iter().enumerate() {
        if let Some(wi) = nodes.iter().position(|n| n.id == h.widget_id) {
            edges.push(SignalEdge { widget_idx: wi, handler_idx: hi });
        }
    }
    edges
}

// ── Layout algorithm ──────────────────────────────────────────────────────────

fn layout_nodes(nodes: &mut Vec<WidgetNode>, handlers: &mut Vec<HandlerNode>) {
    if nodes.is_empty() { return; }

    // Find forest roots (nodes with no parent)
    let roots: Vec<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].parent.is_none())
        .collect();

    let mut x_cursor = 0.0f64;
    for root_idx in roots {
        let sw = subtree_width(nodes, root_idx);
        assign_positions(nodes, root_idx, 0, x_cursor);
        x_cursor += sw + SIBLING_GAP * 2.0;
    }

    // Place handler nodes in a row below the widget tree
    if !handlers.is_empty() {
        let max_y = nodes.iter().map(|n| n.y).fold(0.0f64, f64::max);
        let handler_y = max_y + NODE_H + LEVEL_GAP * 1.5;

        let total_w = handlers.len() as f64 * (NODE_W + SIBLING_GAP) - SIBLING_GAP;
        let tree_cx  = x_cursor / 2.0;
        let hx_start = (tree_cx - total_w / 2.0).max(0.0);

        for (i, h) in handlers.iter_mut().enumerate() {
            h.x = hx_start + i as f64 * (NODE_W + SIBLING_GAP);
            h.y = handler_y;
        }
    }
}

fn subtree_width(nodes: &[WidgetNode], idx: usize) -> f64 {
    if nodes[idx].children.is_empty() {
        return NODE_W;
    }
    let total: f64 = nodes[idx].children.iter()
        .map(|&ci| subtree_width(nodes, ci))
        .sum();
    let gaps = SIBLING_GAP * (nodes[idx].children.len() - 1) as f64;
    (total + gaps).max(NODE_W)
}

fn assign_positions(nodes: &mut Vec<WidgetNode>, idx: usize, depth: usize, x_start: f64) {
    let sw = subtree_width(nodes, idx);
    nodes[idx].x = x_start + (sw - NODE_W) / 2.0;
    nodes[idx].y = depth as f64 * (NODE_H + LEVEL_GAP);
    let mut child_x = x_start;
    let children: Vec<usize> = nodes[idx].children.clone();
    for ci in children {
        let csw = subtree_width(nodes, ci);
        assign_positions(nodes, ci, depth + 1, child_x);
        child_x += csw + SIBLING_GAP;
    }
}

// ── Fit / pan helpers ─────────────────────────────────────────────────────────

fn fit_to_view(inner: &Inner) {
    let nodes    = inner.nodes.borrow();
    let handlers = inner.handlers.borrow();
    if nodes.is_empty() { return; }

    let all_items: Vec<(f64, f64)> = nodes.iter().map(|n| (n.x, n.y))
        .chain(handlers.iter().map(|h| (h.x, h.y)))
        .collect();

    let min_x = all_items.iter().map(|p| p.0).fold(f64::INFINITY,     f64::min);
    let max_x = all_items.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) + NODE_W;
    let min_y = all_items.iter().map(|p| p.1).fold(f64::INFINITY,     f64::min);
    let max_y = all_items.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) + NODE_H;

    let vw = inner.area.allocated_width()  as f64;
    let vh = inner.area.allocated_height() as f64;
    if vw <= 1.0 || vh <= 1.0 { return; }

    let pad   = 40.0;
    let gw    = (max_x - min_x).max(1.0);
    let gh    = (max_y - min_y).max(1.0);
    let scale = ((vw - pad * 2.0) / gw).min((vh - pad * 2.0) / gh).min(1.5).max(0.15);

    let pan_x = (vw - gw * scale) / 2.0 - min_x * scale;
    let pan_y = (vh - gh * scale) / 2.0 - min_y * scale;

    inner.pan.set((pan_x, pan_y));
    inner.scale.set(scale);
}

// ── Hit testing ───────────────────────────────────────────────────────────────

fn hit_test(inner: &Inner, sx: f64, sy: f64) -> Option<usize> {
    let (px, py) = inner.pan.get();
    let scale    = inner.scale.get();
    let gx = (sx - px) / scale;
    let gy = (sy - py) / scale;

    let nodes    = inner.nodes.borrow();
    let handlers = inner.handlers.borrow();
    let nlen = nodes.len();

    for (i, h) in handlers.iter().enumerate() {
        if gx >= h.x && gx <= h.x + NODE_W && gy >= h.y && gy <= h.y + NODE_H {
            return Some(nlen + i);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        if gx >= n.x && gx <= n.x + NODE_W && gy >= n.y && gy <= n.y + NODE_H {
            return Some(i);
        }
    }
    None
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw_graph(inner: &Inner, cr: &gtk4::cairo::Context, vw: f64, vh: f64) {
    // Background
    cr.set_source_rgb(0.106, 0.114, 0.149);
    cr.rectangle(0.0, 0.0, vw, vh);
    let _ = cr.fill();

    let nodes    = inner.nodes.borrow();
    let handlers = inner.handlers.borrow();
    let edges    = inner.edges.borrow();
    let selected = inner.selected.get();
    let nlen     = nodes.len();

    if nodes.is_empty() {
        cr.set_source_rgb(0.549, 0.561, 0.678);
        cr.set_font_size(13.0);
        let msg = "Open a .ui file to see the widget node graph";
        cr.move_to(vw / 2.0 - 155.0, vh / 2.0);
        let _ = cr.show_text(msg);
        return;
    }

    let (px, py) = inner.pan.get();
    let scale    = inner.scale.get();

    let _ = cr.save();
    cr.translate(px, py);
    cr.scale(scale, scale);

    // ── Containment edges (parent → child, solid) ─────────────────────────
    cr.set_line_width(1.5 / scale);
    cr.set_source_rgba(0.549, 0.561, 0.678, 0.35);
    cr.set_dash(&[], 0.0);

    for node in nodes.iter() {
        for &ci in &node.children {
            let child = &nodes[ci];
            let x1 = node.x  + NODE_W / 2.0;
            let y1 = node.y  + NODE_H;
            let x2 = child.x + NODE_W / 2.0;
            let y2 = child.y;
            let ctrl = (y2 - y1) * 0.5;
            cr.move_to(x1, y1);
            cr.curve_to(x1, y1 + ctrl, x2, y2 - ctrl, x2, y2);
            let _ = cr.stroke();
        }
    }

    // ── Signal edges (widget → handler, dashed pink) ──────────────────────
    cr.set_source_rgba(0.824, 0.478, 0.600, 0.65);
    cr.set_dash(&[7.0 / scale, 4.0 / scale], 0.0);
    cr.set_line_width(1.5 / scale);

    for edge in edges.iter() {
        if let (Some(wn), Some(hn)) =
            (nodes.get(edge.widget_idx), handlers.get(edge.handler_idx))
        {
            let x1 = wn.x + NODE_W / 2.0;
            let y1 = wn.y + NODE_H;
            let x2 = hn.x + NODE_W / 2.0;
            let y2 = hn.y;
            let midy = (y1 + y2) / 2.0;
            cr.move_to(x1, y1);
            cr.curve_to(x1, midy, x2, midy, x2, y2);
            let _ = cr.stroke();
        }
    }

    cr.set_dash(&[], 0.0);

    // ── Widget nodes ──────────────────────────────────────────────────────
    for (i, node) in nodes.iter().enumerate() {
        draw_widget_node(cr, node, selected == Some(i), scale);
    }

    // ── Handler nodes ─────────────────────────────────────────────────────
    for (i, h) in handlers.iter().enumerate() {
        draw_handler_node(cr, h, selected == Some(nlen + i), scale);
    }

    let _ = cr.restore();
}

fn draw_widget_node(
    cr: &gtk4::cairo::Context,
    node: &WidgetNode,
    selected: bool,
    scale: f64,
) {
    let (fr, fg, fb) = widget_color(&node.class_name);

    // Fill (dark tint of accent colour)
    cr.set_source_rgb(fr * 0.18, fg * 0.18, fb * 0.18);
    rounded_rect(cr, node.x, node.y, NODE_W, NODE_H, 7.0);
    let _ = cr.fill();

    // Border
    if selected {
        cr.set_source_rgb(0.984, 0.867, 0.400);   // yellow highlight
        cr.set_line_width(2.5 / scale);
    } else {
        cr.set_source_rgb(fr, fg, fb);
        cr.set_line_width(1.5 / scale);
    }
    rounded_rect(cr, node.x, node.y, NODE_W, NODE_H, 7.0);
    let _ = cr.stroke();

    // Class name (accent colour)
    cr.set_source_rgb(fr, fg, fb);
    cr.set_font_size(11.5);
    cr.move_to(node.x + 9.0, node.y + 20.0);
    let _ = cr.show_text(&node.class_name);

    // Widget id (light)
    cr.set_source_rgb(0.898, 0.902, 0.957);
    cr.set_font_size(10.0);
    let id_str = if node.id.is_empty() { "(no id)" } else { &node.id };
    cr.move_to(node.x + 9.0, node.y + 37.0);
    let _ = cr.show_text(&trunc(id_str, 21));
}

fn draw_handler_node(
    cr: &gtk4::cairo::Context,
    h: &HandlerNode,
    selected: bool,
    scale: f64,
) {
    let (fr, fg, fb) = (0.953_f64, 0.443, 0.671);  // pink / flamingo

    cr.set_source_rgb(fr * 0.18, fg * 0.18, fb * 0.18);
    rounded_rect(cr, h.x, h.y, NODE_W, NODE_H, 7.0);
    let _ = cr.fill();

    if selected {
        cr.set_source_rgb(0.984, 0.867, 0.400);
        cr.set_line_width(2.5 / scale);
    } else {
        cr.set_source_rgb(fr, fg, fb);
        cr.set_line_width(1.5 / scale);
    }
    rounded_rect(cr, h.x, h.y, NODE_W, NODE_H, 7.0);
    let _ = cr.stroke();

    // Signal name
    cr.set_source_rgb(fr, fg, fb);
    cr.set_font_size(11.5);
    cr.move_to(h.x + 9.0, h.y + 20.0);
    let _ = cr.show_text(&format!(":: {}", h.signal));

    // Widget id + line number
    cr.set_source_rgb(0.898, 0.902, 0.957);
    cr.set_font_size(10.0);
    cr.move_to(h.x + 9.0, h.y + 37.0);
    let lbl = format!("{}  (ln {})", trunc(&h.widget_id, 13), h.rs_line);
    let _ = cr.show_text(&lbl);
}

// ── Cairo helpers ─────────────────────────────────────────────────────────────

fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + r,     y + r,     r, PI,        3.0 * PI / 2.0);
    cr.arc(x + w - r, y + r,     r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0,       PI / 2.0);
    cr.arc(x + r,     y + h - r, r, PI / 2.0,  PI);
    cr.close_path();
}

fn trunc(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let t: String = chars[..max.saturating_sub(1)].iter().collect();
        format!("{}…", t)
    }
}

fn widget_color(class: &str) -> (f64, f64, f64) {
    match class {
        c if c.contains("Button")    => (0.537, 0.706, 0.980), // blue
        c if c.contains("Label")     => (0.651, 0.890, 0.631), // green
        c if c.contains("Box")       => (0.980, 0.706, 0.529), // peach
        c if c.contains("Grid")      => (0.976, 0.886, 0.686), // yellow
        c if c.contains("Entry")     => (0.796, 0.651, 0.969), // mauve
        c if c.contains("Separator") => (0.706, 0.745, 0.843), // overlay
        c if c.contains("Paned")     => (0.561, 0.871, 0.922), // sky
        c if c.contains("Stack")     => (0.678, 0.925, 0.741), // teal
        c if c.contains("Notebook")  => (0.953, 0.706, 0.780), // flamingo
        c if c.contains("Scrolled")  => (0.729, 0.761, 0.863), // subtext
        c if c.contains("Frame")     => (0.808, 0.839, 0.957), // lavender
        c if c.contains("Switch")    => (0.537, 0.706, 0.980), // blue
        c if c.contains("Check")     => (0.651, 0.890, 0.631), // green
        c if c.contains("Combo")     => (0.796, 0.651, 0.969), // mauve
        c if c.contains("Text")      => (0.729, 0.761, 0.863), // subtext
        _                            => (0.649, 0.661, 0.773), // overlay2
    }
}
