use sourceview5::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, Entry, Label, Orientation, SearchBar, SearchEntry,
};
use sourceview5::{Buffer, SearchContext, SearchSettings};

#[derive(Clone)]
pub struct FindBar {
    pub widget:    SearchBar,
    search_ctx:    std::rc::Rc<std::cell::RefCell<Option<SearchContext>>>,
    entry:         SearchEntry,
    replace_entry: Entry,
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

        let search_ctx: std::rc::Rc<std::cell::RefCell<Option<SearchContext>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));

        {
            let ctx_rc = search_ctx.clone();
            next_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() { find_next(ctx); }
            });
        }
        {
            let ctx_rc = search_ctx.clone();
            prev_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() { find_prev(ctx); }
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
            let ctx_rc = search_ctx.clone();
            entry.connect_activate(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() { find_next(ctx); }
            });
        }

        {
            let ctx_rc = search_ctx.clone();
            let rep_e  = replace_entry.clone();
            replace_btn.connect_clicked(move |_| {
                if let Some(ctx) = ctx_rc.borrow().as_ref() {
                    replace_current(ctx, &rep_e.text());
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

        let fb = Self { widget: bar, search_ctx, entry: entry.clone(), replace_entry };
        fb.widget.connect_entry(&fb.entry);
        fb
    }

    pub fn set_buffer(&self, buffer: &Buffer) {
        let settings = SearchSettings::new();
        settings.set_wrap_around(true);
        settings.set_case_sensitive(false);
        let ctx = SearchContext::new(buffer, Some(&settings));
        let text = self.entry.text().to_string();
        if !text.is_empty() {
            settings.set_search_text(Some(&text));
        }
        *self.search_ctx.borrow_mut() = Some(ctx);
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
    ctx_rc: &std::rc::Rc<std::cell::RefCell<Option<SearchContext>>>,
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
        let count = ctx.occurrences_count();
        let match_text = if text.is_empty() {
            String::new()
        } else if count > 0 {
            format!("{} matches", count)
        } else {
            "No matches".to_string()
        };
        lbl.set_text(&match_text);
    }
}

fn find_next(ctx: &SearchContext) {
    let buf = ctx.buffer();
    let mark = buf.get_insert();
    let cursor = buf.iter_at_mark(&mark);
    if let Some((start, end, _)) = ctx.forward(&cursor) {
        buf.select_range(&start, &end);
    }
}

fn find_prev(ctx: &SearchContext) {
    let buf = ctx.buffer();
    let mark = buf.get_insert();
    let cursor = buf.iter_at_mark(&mark);
    if let Some((start, end, _)) = ctx.backward(&cursor) {
        buf.select_range(&start, &end);
    }
}

fn replace_current(ctx: &SearchContext, replacement: &str) {
    let buf = ctx.buffer();
    if let Some((mut ms, mut me)) = buf.selection_bounds() {
        let _ = ctx.replace(&mut ms, &mut me, replacement);
        find_next(ctx);
    } else {
        find_next(ctx);
    }
}
