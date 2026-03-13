use sourceview5::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, Entry, Label, Orientation, SearchBar, SearchEntry,
};
use sourceview5::{Buffer, SearchContext, SearchSettings, View};
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone)]
pub struct FindBar {
    pub widget:    SearchBar,
    search_ctx:    Rc<RefCell<Option<SearchContext>>>,
    current_view:  Rc<RefCell<Option<View>>>,
    entry:         SearchEntry,
    replace_entry: Entry,
    case_btn:      CheckButton,
    regex_btn:     CheckButton,
    match_lbl:     Label,
}

impl FindBar {
    pub fn new() -> Self {
        let bar = SearchBar::new();
        bar.add_css_class("editor-find-bar");
        bar.set_show_close_button(true);

        let vbox = GtkBox::new(Orientation::Vertical, 4);
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);
        vbox.set_margin_top(4);
        vbox.set_margin_bottom(4);

        let find_row = GtkBox::new(Orientation::Horizontal, 6);

        let entry = SearchEntry::new();
        entry.set_placeholder_text(Some("Find…"));
        entry.set_hexpand(true);

        let prev_btn = Button::with_label("◀");
        let next_btn = Button::with_label("▶");

        let case_btn = CheckButton::with_label("Aa");
        case_btn.set_tooltip_text(Some("Case sensitive"));

        let regex_btn = CheckButton::with_label(".*");
        regex_btn.set_tooltip_text(Some("Regular expression"));

        let match_lbl = Label::new(Some(""));
        match_lbl.add_css_class("editor-statusbar-item");

        find_row.append(&entry);
        find_row.append(&prev_btn);
        find_row.append(&next_btn);
        find_row.append(&case_btn);
        find_row.append(&regex_btn);
        find_row.append(&match_lbl);

        let replace_row = GtkBox::new(Orientation::Horizontal, 6);

        let replace_entry = Entry::new();
        replace_entry.set_placeholder_text(Some("Replace with…"));
        replace_entry.set_hexpand(true);

        let replace_btn     = Button::with_label("Replace");
        let replace_all_btn = Button::with_label("Replace All");

        replace_row.append(&replace_entry);
        replace_row.append(&replace_btn);
        replace_row.append(&replace_all_btn);

        vbox.append(&find_row);
        vbox.append(&replace_row);
        bar.set_child(Some(&vbox));

        let search_ctx:   Rc<RefCell<Option<SearchContext>>> = Rc::new(RefCell::new(None));
        let current_view: Rc<RefCell<Option<View>>>          = Rc::new(RefCell::new(None));

        {
            let ctx_rc = search_ctx.clone();
            let view_rc = current_view.clone();
            next_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    find_next(ctx, view_rc.borrow().as_ref());
                }
            });
        }
        {
            let ctx_rc = search_ctx.clone();
            let view_rc = current_view.clone();
            prev_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    find_prev(ctx, view_rc.borrow().as_ref());
                }
            });
        }

        {
            let ctx_rc  = search_ctx.clone();
            let lbl     = match_lbl.clone();
            let case_c  = case_btn.clone();
            let regex_c = regex_btn.clone();
            entry.connect_search_changed(move |e| {
                update_search(&ctx_rc, &e.text(), case_c.is_active(), regex_c.is_active(), &lbl);
            });
        }
        {
            let ctx_rc  = search_ctx.clone();
            let entry_c = entry.clone();
            let lbl     = match_lbl.clone();
            let regex_c = regex_btn.clone();
            case_btn.connect_toggled(move |c| {
                update_search(&ctx_rc, &entry_c.text(), c.is_active(), regex_c.is_active(), &lbl);
            });
        }
        {
            let ctx_rc  = search_ctx.clone();
            let entry_c = entry.clone();
            let lbl     = match_lbl.clone();
            let case_c  = case_btn.clone();
            regex_btn.connect_toggled(move |r| {
                update_search(&ctx_rc, &entry_c.text(), case_c.is_active(), r.is_active(), &lbl);
            });
        }

        {
            let ctx_rc  = search_ctx.clone();
            let view_rc = current_view.clone();
            entry.connect_activate(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    find_next(ctx, view_rc.borrow().as_ref());
                }
            });
        }

        {
            let ctx_rc = search_ctx.clone();
            let rep_e  = replace_entry.clone();
            let view_rc = current_view.clone();
            replace_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    replace_current(ctx, &rep_e.text(), view_rc.borrow().as_ref());
                }
            });
        }

        {
            let ctx_rc = search_ctx.clone();
            let rep_e  = replace_entry.clone();
            replace_all_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    let _ = ctx.replace_all(&rep_e.text());
                }
            });
        }

        let fb = Self {
            widget: bar,
            search_ctx,
            current_view,
            entry: entry.clone(),
            replace_entry,
            case_btn,
            regex_btn,
            match_lbl,
        };
        fb.widget.connect_entry(&fb.entry);
        fb
    }

    /// Call this whenever the active editor tab changes.
    pub fn set_view(&self, view: &View) {
        *self.current_view.borrow_mut() = Some(view.clone());

        let buf = view.buffer()
            .downcast::<Buffer>()
            .expect("FindBar: view buffer is always sourceview5::Buffer");

        let settings = SearchSettings::new();
        settings.set_wrap_around(true);
        // Preserve current checkbox state across tab switches
        settings.set_case_sensitive(self.case_btn.is_active());
        settings.set_regex_enabled(self.regex_btn.is_active());

        let text = self.entry.text().to_string();
        if !text.is_empty() {
            settings.set_search_text(Some(&text));
        }

        let ctx = SearchContext::new(&buf, Some(&settings));

        // Keep match count label live — GtkSourceView updates this asynchronously
        {
            let lbl  = self.match_lbl.clone();
            let ent  = self.entry.clone();
            ctx.connect_notify_local(Some("occurrences-count"), move |c, _| {
                let count = c.occurrences_count();
                let query = ent.text();
                lbl.set_text(&if query.is_empty() {
                    String::new()
                } else if count > 0 {
                    format!("{} matches", count)
                } else {
                    "No matches".to_string()
                });
            });
        }

        *self.search_ctx.borrow_mut() = Some(ctx);
    }

    /// Backwards-compat shim — callers that only have a Buffer can still call this.
    /// Prefer `set_view` so scrolling works correctly.
    pub fn set_buffer(&self, buffer: &Buffer) {
        let settings = SearchSettings::new();
        settings.set_wrap_around(true);
        settings.set_case_sensitive(self.case_btn.is_active());
        settings.set_regex_enabled(self.regex_btn.is_active());
        let text = self.entry.text().to_string();
        if !text.is_empty() {
            settings.set_search_text(Some(&text));
        }
        let ctx = SearchContext::new(buffer, Some(&settings));
        {
            let lbl = self.match_lbl.clone();
            let ent = self.entry.clone();
            ctx.connect_notify_local(Some("occurrences-count"), move |c, _| {
                let count = c.occurrences_count();
                let query = ent.text();
                lbl.set_text(&if query.is_empty() {
                    String::new()
                } else if count > 0 {
                    format!("{} matches", count)
                } else {
                    "No matches".to_string()
                });
            });
        }
        *self.search_ctx.borrow_mut() = Some(ctx);
        *self.current_view.borrow_mut() = None;
    }

    pub fn reveal(&self) {
        self.widget.set_search_mode(true);
        self.entry.grab_focus();
    }

    pub fn reveal_replace(&self) {
        self.widget.set_search_mode(true);
        self.replace_entry.grab_focus();
    }

    pub fn hide(&self)   { self.widget.set_search_mode(false); }

    pub fn toggle(&self) {
        if self.widget.is_search_mode() { self.hide(); } else { self.reveal(); }
    }
}

fn update_search(
    ctx_rc: &Rc<RefCell<Option<SearchContext>>>,
    text: &str,
    case_sensitive: bool,
    use_regex: bool,
    lbl: &Label,
) {
    if let Some(ctx) = ctx_rc.borrow().as_ref() {
        let s = ctx.settings();
        s.set_case_sensitive(case_sensitive);
        s.set_regex_enabled(use_regex);
        s.set_search_text(if text.is_empty() { None } else { Some(text) });
        // Count will arrive via notify::occurrences-count; show "…" while waiting
        if !text.is_empty() {
            let count = ctx.occurrences_count();
            lbl.set_text(&if count > 0 {
                format!("{} matches", count)
            } else {
                "…".to_string()
            });
        } else {
            lbl.set_text("");
        }
    }
}

fn find_next(ctx: &SearchContext, view: Option<&View>) {
    let buf = ctx.buffer();
    // Start from the END of any current selection so we advance past it,
    // not re-find the same match.
    let from = buf.selection_bounds()
        .map(|(_, end)| end)
        .unwrap_or_else(|| buf.iter_at_mark(&buf.get_insert()));
    if let Some((start, end, _)) = ctx.forward(&from) {
        buf.select_range(&start, &end);
        if let Some(v) = view {
            v.scroll_to_iter(&mut start.clone(), 0.1, true, 0.5, 0.5);
        }
    }
}

fn find_prev(ctx: &SearchContext, view: Option<&View>) {
    let buf = ctx.buffer();
    let cursor = buf.iter_at_mark(&buf.get_insert());
    if let Some((start, end, _)) = ctx.backward(&cursor) {
        buf.select_range(&start, &end);
        if let Some(v) = view {
            v.scroll_to_iter(&mut start.clone(), 0.1, true, 0.5, 0.5);
        }
    }
}

fn replace_current(ctx: &SearchContext, replacement: &str, view: Option<&View>) {
    let buf = ctx.buffer();
    if let Some((mut ms, mut me)) = buf.selection_bounds() {
        let _ = ctx.replace(&mut ms, &mut me, replacement);
    }
    find_next(ctx, view);
}
