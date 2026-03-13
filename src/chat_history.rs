//! chat_history.rs — Session history panel for AI chat code blocks.
//!
//! Collects every diff / xml / rust block returned by any AI panel during
//! the session and presents them in a scrollable list with re-apply buttons.
//! Added as a third tab in right_nb alongside Properties and AI Chat.

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Orientation, ScrolledWindow, Separator};
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn current_time() -> String {
    match glib::DateTime::now_local() {
        Ok(dt) => format!("{:02}:{:02}", dt.hour(), dt.minute()),
        Err(_) => "--:--".to_string(),
    }
}

/// Build a short human-readable preview for a code block.
fn make_preview(lang: &str, content: &str) -> String {
    if matches!(lang, "diff" | "patch") {
        // Show target filename from +++ header
        for line in content.lines() {
            if line.starts_with("+++ ") {
                let path = line.strip_prefix("+++ ").unwrap_or("").trim();
                let path = path.strip_prefix("b/").unwrap_or(path);
                if path != "/dev/null" && !path.is_empty() {
                    return path.to_string();
                }
            }
        }
        return "patch".to_string();
    }
    // For code blocks: first non-empty non-comment line, capped at 60 chars
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('#') || t.starts_with("<!--") {
            continue;
        }
        return if t.len() > 60 {
            format!("{}…", &t[..60])
        } else {
            t.to_string()
        };
    }
    if lang.is_empty() { "block".to_string() } else { lang.to_string() }
}

// ── Panel ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ChatHistoryPanel {
    pub widget:      GtkBox,
    entries_box:     GtkBox,
    scroll:          ScrolledWindow,
    empty_lbl:       Label,
    on_view_diff_cb:        Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_apply_xml_cb:        Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_apply_rs_cb:         Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_apply_strreplace_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
}

impl ChatHistoryPanel {
    pub fn new() -> Self {
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        // ── Header ────────────────────────────────────────────────────
        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_margin_start(8);
        header.set_margin_end(8);
        header.set_margin_top(6);
        header.set_margin_bottom(4);

        let title = Label::new(Some("Session History"));
        title.add_css_class("heading");
        title.set_hexpand(true);
        title.set_halign(gtk4::Align::Start);

        let clear_btn = Button::with_label("Clear");
        clear_btn.add_css_class("flat");
        clear_btn.set_tooltip_text(Some("Clear history"));

        header.append(&title);
        header.append(&clear_btn);

        // ── Entries list ──────────────────────────────────────────────
        let entries_box = GtkBox::new(Orientation::Vertical, 6);
        entries_box.set_margin_start(8);
        entries_box.set_margin_end(8);
        entries_box.set_margin_top(8);
        entries_box.set_margin_bottom(8);

        let empty_lbl = Label::new(Some("No code blocks yet.\nBlocks from AI responses appear here."));
        empty_lbl.set_justify(gtk4::Justification::Center);
        empty_lbl.add_css_class("dim-label");
        empty_lbl.set_vexpand(true);
        empty_lbl.set_valign(gtk4::Align::Center);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&entries_box)
            .build();

        widget.append(&header);
        widget.append(&Separator::new(Orientation::Horizontal));
        widget.append(&scroll);

        let panel = ChatHistoryPanel {
            widget,
            entries_box: entries_box.clone(),
            scroll,
            empty_lbl: empty_lbl.clone(),
            on_view_diff_cb:        Rc::new(RefCell::new(None)),
            on_apply_xml_cb:        Rc::new(RefCell::new(None)),
            on_apply_rs_cb:         Rc::new(RefCell::new(None)),
            on_apply_strreplace_cb: Rc::new(RefCell::new(None)),
        };

        // Show empty placeholder initially
        entries_box.append(&empty_lbl);

        // Wire Clear
        {
            let p = panel.clone();
            clear_btn.connect_clicked(move |_| {
                while let Some(child) = p.entries_box.first_child() {
                    p.entries_box.remove(&child);
                }
                p.entries_box.append(&p.empty_lbl);
                p.empty_lbl.set_visible(true);
            });
        }

        panel
    }

    // ── Callback registration ─────────────────────────────────────────────────

    pub fn on_view_diff<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_view_diff_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_xml<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_xml_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_rs<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_rs_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_strreplace<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_strreplace_cb.borrow_mut() = Some(Box::new(cb));
    }

    // ── Entry creation ────────────────────────────────────────────────────────

    /// Add a code block entry.  `source` is e.g. "Claude Code" or "AI Chat".
    /// `display_name` overrides the auto-generated preview (use the filename).
    pub fn add_entry(&self, source: &str, lang: &str, display_name: Option<String>, content: String) {
        // Hide the empty placeholder on first real entry
        self.empty_lbl.set_visible(false);

        let time    = current_time();
        let preview = display_name.unwrap_or_else(|| make_preview(lang, &content));

        let is_diff      = matches!(lang, "diff" | "patch");
        let is_xml       = matches!(lang, "xml" | "ui");
        let is_rust      = matches!(lang, "rust" | "rs");
        let is_strreplace = matches!(lang, "strreplace" | "edit");

        // ── Entry card ────────────────────────────────────────────────
        let card = GtkBox::new(Orientation::Vertical, 4);
        card.add_css_class("chat-code-block");
        card.set_margin_top(2);

        // Info row: time · source · lang badge · preview
        let info_row = GtkBox::new(Orientation::Horizontal, 6);
        info_row.set_margin_start(8);
        info_row.set_margin_end(8);
        info_row.set_margin_top(6);

        let time_lbl = Label::new(Some(&time));
        time_lbl.add_css_class("dim-label");
        time_lbl.add_css_class("chat-code-lang");

        let src_lbl = Label::new(Some(source));
        src_lbl.add_css_class("chat-code-lang");

        let lang_badge = if lang.is_empty() { "block".to_string() } else { lang.to_string() };
        let lang_lbl = Label::new(Some(&lang_badge));
        lang_lbl.add_css_class("chat-code-lang");

        let prev_lbl = Label::new(Some(&preview));
        prev_lbl.set_hexpand(true);
        prev_lbl.set_halign(gtk4::Align::Start);
        prev_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        info_row.append(&time_lbl);
        info_row.append(&Label::new(Some("·")));
        info_row.append(&src_lbl);
        info_row.append(&Label::new(Some("·")));
        info_row.append(&lang_lbl);
        info_row.append(&Label::new(Some("·")));
        info_row.append(&prev_lbl);
        card.append(&info_row);

        // Action buttons row
        let btn_row = GtkBox::new(Orientation::Horizontal, 4);
        btn_row.set_margin_start(8);
        btn_row.set_margin_end(8);
        btn_row.set_margin_bottom(6);
        btn_row.set_margin_top(2);

        if is_diff {
            let label = format!("View Diff — {}", preview);
            let btn = Button::with_label(&label);
            btn.add_css_class("success");
            let cb  = self.on_view_diff_cb.clone();
            let c   = content.clone();
            btn.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() { f(&c); }
            });
            btn_row.append(&btn);
        } else if is_xml {
            let label = format!("Apply to {}", preview);
            let btn = Button::with_label(&label);
            btn.add_css_class("suggested-action");
            let cb  = self.on_apply_xml_cb.clone();
            let c   = content.clone();
            btn.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() { f(&c); }
            });
            btn_row.append(&btn);
        } else if is_rust {
            let label = format!("Apply to {}", preview);
            let btn = Button::with_label(&label);
            btn.add_css_class("suggested-action");
            let cb  = self.on_apply_rs_cb.clone();
            let c   = content.clone();
            btn.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() { f(&c); }
            });
            btn_row.append(&btn);
        } else if is_strreplace {
            let name = crate::strreplace::display_name(&content);
            let label = format!("Apply changes to {}", name);
            let btn = Button::with_label(&label);
            btn.add_css_class("success");
            let cb  = self.on_apply_strreplace_cb.clone();
            let c   = content.clone();
            btn.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() { f(&c); }
            });
            btn_row.append(&btn);
        } else {
            // Generic copy button for unknown block types
            let btn = Button::builder()
                .icon_name("edit-copy-symbolic")
                .has_frame(false)
                .tooltip_text("Copy block")
                .build();
            btn.add_css_class("flat");
            btn.connect_clicked(move |_| {
                if let Some(d) = gtk4::gdk::Display::default() {
                    d.clipboard().set_text(&content);
                }
            });
            btn_row.append(&btn);
        }

        card.append(&btn_row);

        // Prepend so newest entries appear at the top
        self.entries_box.prepend(&card);
        self.scroll_to_top();
    }

    fn scroll_to_top(&self) {
        let adj = self.scroll.vadjustment();
        glib::idle_add_local_once(move || { adj.set_value(0.0); });
    }
}
