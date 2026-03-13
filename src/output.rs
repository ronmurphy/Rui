use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, Notebook, Orientation, ScrolledWindow, TextBuffer, TextTag,
    TextView,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use crate::diagnostics::{Diagnostic, DiagLevel};

/// Callback invoked when the user clicks an error location in the Run output.
pub type ErrorClickCb = Rc<RefCell<Option<Box<dyn Fn(PathBuf, i32, i32)>>>>;
pub type AskAiCb      = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;
pub type SearchReqCb  = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;

/// A single ripgrep match — file path, 1-based line/col, matched line text.
#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub file: PathBuf,
    pub line: u64,
    pub col:  u64,
    pub text: String,
}

#[derive(Clone)]
pub struct OutputPanel {
    pub widget:  GtkBox,
    stack:       gtk4::Stack,
    run_buf:     TextBuffer,
    run_view:    TextView,
    out_buf:     TextBuffer,
    prob_buf:    TextBuffer,
    prob_view:   TextView,
    prob_error_tag:   TextTag,
    prob_warning_tag: TextTag,
    prob_link_tag:    TextTag,
    error_tag:   TextTag,
    success_tag: TextTag,
    link_tag:    TextTag,
    /// Called with (file_path, line, col) when user clicks an error location.
    on_error_click: ErrorClickCb,
    /// Called with the extracted error text when the robot button is clicked.
    on_ask_ai: AskAiCb,
    /// Accumulated error lines (populated by append_run_error).
    errors: Rc<RefCell<Vec<String>>>,
    // ── Search tab ────────────────────────────────────────────────────────────
    search_buf:      TextBuffer,
    search_view:     TextView,
    search_entry:    Entry,
    search_link_tag: TextTag,
    search_file_tag: TextTag,
    on_search_request: SearchReqCb,
}

impl OutputPanel {
    pub fn new() -> Self {
        let vbox = GtkBox::new(Orientation::Vertical, 0);

        let stack = gtk4::Stack::new();
        stack.set_vexpand(true);
        stack.set_hexpand(true);

        let (out_view, out_buf) = make_text_view();
        let (prob_view, prob_buf) = make_text_view();
        let (run_view, run_buf) = make_text_view();

        let error_tag = TextTag::builder()
            .name("error")
            .foreground("#f38ba8")
            .build();
        let success_tag = TextTag::builder()
            .name("success")
            .foreground("#a6e3a1")
            .build();
        let link_tag = TextTag::builder()
            .name("link")
            .foreground("#89b4fa")
            .underline(gtk4::pango::Underline::Single)
            .build();
        run_buf.tag_table().add(&error_tag);
        run_buf.tag_table().add(&success_tag);
        run_buf.tag_table().add(&link_tag);

        // Problems tab tags
        let prob_error_tag = TextTag::builder()
            .name("prob-error")
            .foreground("#f38ba8")
            .weight(700)
            .build();
        let prob_warning_tag = TextTag::builder()
            .name("prob-warning")
            .foreground("#f9e2af")
            .weight(700)
            .build();
        let prob_link_tag = TextTag::builder()
            .name("prob-link")
            .foreground("#89b4fa")
            .underline(gtk4::pango::Underline::Single)
            .build();
        prob_buf.tag_table().add(&prob_error_tag);
        prob_buf.tag_table().add(&prob_warning_tag);
        prob_buf.tag_table().add(&prob_link_tag);

        // ── Search tab ────────────────────────────────────────────────────────
        let (search_view, search_buf) = make_text_view();

        let search_file_tag = TextTag::builder()
            .name("search-file")
            .foreground("#cba6f7")
            .weight(700)
            .build();
        let search_link_tag = TextTag::builder()
            .name("search-link")
            .foreground("#89b4fa")
            .underline(gtk4::pango::Underline::Single)
            .build();
        search_buf.tag_table().add(&search_file_tag);
        search_buf.tag_table().add(&search_link_tag);

        let search_entry = Entry::builder()
            .placeholder_text("Search in project…")
            .hexpand(true)
            .build();
        let search_go_btn = Button::with_label("Search");
        let search_bar = GtkBox::new(Orientation::Horizontal, 4);
        search_bar.set_margin_start(4);
        search_bar.set_margin_end(4);
        search_bar.set_margin_top(4);
        search_bar.set_margin_bottom(2);
        search_bar.append(&search_entry);
        search_bar.append(&search_go_btn);

        let search_tab_box = GtkBox::new(Orientation::Vertical, 0);
        search_tab_box.append(&search_bar);
        search_tab_box.append(&wrap_scroll(search_view.clone()));

        let on_search_request: SearchReqCb = Rc::new(RefCell::new(None));
        {
            let cb = on_search_request.clone();
            let ent = search_entry.clone();
            search_go_btn.connect_clicked(move |_| {
                let q = ent.text().to_string();
                if !q.is_empty() {
                    if let Some(f) = cb.borrow().as_ref() { f(&q); }
                }
            });
        }
        {
            let cb = on_search_request.clone();
            search_entry.connect_activate(move |e| {
                let q = e.text().to_string();
                if !q.is_empty() {
                    if let Some(f) = cb.borrow().as_ref() { f(&q); }
                }
            });
        }
        // ─────────────────────────────────────────────────────────────────────

        stack.add_titled(&wrap_scroll(out_view.clone()), Some("output"), "Output");
        stack.add_titled(&wrap_scroll(prob_view.clone()), Some("problems"), "Problems");
        stack.add_titled(&wrap_scroll(run_view.clone()), Some("run"), "Run");
        stack.add_titled(&search_tab_box, Some("search"), "Search");

        let switcher = gtk4::StackSwitcher::new();
        switcher.set_stack(Some(&stack));

        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_margin_start(4);
        header.set_margin_end(4);
        header.set_margin_top(4);
        header.set_margin_bottom(4);
        header.append(&switcher);

        // Robot button — send errors to active AI panel
        let robot_btn = Button::with_label("\u{F544}"); // nf-fa-robot
        robot_btn.add_css_class("flat");
        robot_btn.add_css_class("nf");
        robot_btn.set_tooltip_text(Some("Send errors to AI"));
        robot_btn.set_hexpand(true);
        robot_btn.set_halign(gtk4::Align::End);

        // Close button in the tab bar area
        let close_btn = Button::builder()
            .icon_name("window-close-symbolic")
            .has_frame(false)
            .tooltip_text("Close panel")
            .build();
        close_btn.add_css_class("flat");
        close_btn.set_hexpand(false);
        {
            let w = vbox.clone();
            close_btn.connect_clicked(move |_| {
                w.set_visible(false);
            });
        }
        header.append(&robot_btn);
        header.append(&close_btn);

        vbox.append(&header);
        vbox.append(&stack);

        let on_error_click: ErrorClickCb = Rc::new(RefCell::new(None));
        let on_ask_ai: AskAiCb = Rc::new(RefCell::new(None));
        let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        // Wire robot button → collect errors → call on_ask_ai
        {
            let errors_rc = errors.clone();
            let cb = on_ask_ai.clone();
            robot_btn.connect_clicked(move |_| {
                let lines = errors_rc.borrow();
                if lines.is_empty() { return; }
                let text = lines.join("\n");
                if let Some(f) = cb.borrow().as_ref() {
                    f(&text);
                }
            });
        }

        // Wire click handler on the Run text view
        wire_click_handler(&run_view, &run_buf, "link", &on_error_click);

        // Wire click handler on the Problems text view
        wire_click_handler(&prob_view, &prob_buf, "prob-link", &on_error_click);

        // Wire click handler on the Search text view
        wire_click_handler_custom(&search_view, &search_buf, "search-link", &on_error_click, parse_search_location);

        Self {
            widget: vbox,
            stack,
            run_view,
            run_buf,
            out_buf,
            prob_buf,
            prob_view,
            prob_error_tag,
            prob_warning_tag,
            prob_link_tag,
            error_tag,
            success_tag,
            link_tag,
            on_error_click,
            on_ask_ai,
            errors,
            search_buf,
            search_view,
            search_entry,
            search_link_tag,
            search_file_tag,
            on_search_request,
        }
    }

    /// Set the callback for error-location clicks.
    pub fn set_on_error_click<F: Fn(PathBuf, i32, i32) + 'static>(&self, f: F) {
        *self.on_error_click.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked when the robot button is clicked.
    /// Receives the accumulated error text from the current run.
    pub fn set_on_ask_ai<F: Fn(&str) + 'static>(&self, f: F) {
        *self.on_ask_ai.borrow_mut() = Some(Box::new(f));
    }

    pub fn append_run_line(&self, text: &str) {
        append_line(&self.run_buf, text, None::<&TextTag>);
        self.scroll_run_to_bottom();
        self.stack.set_visible_child_name("run");
    }

    pub fn append_run_error(&self, text: &str) {
        // If this is a source-location line (e.g. " --> src/main.rs:15:9"),
        // tag it as a clickable link instead of plain error text.
        if is_error_location(text) {
            append_line(&self.run_buf, text, Some(&self.link_tag));
        } else {
            append_line(&self.run_buf, text, Some(&self.error_tag));
        }
        self.errors.borrow_mut().push(text.to_string());
        self.scroll_run_to_bottom();
        self.stack.set_visible_child_name("run");
    }

    pub fn append_run_success(&self, text: &str) {
        append_line(&self.run_buf, text, Some(&self.success_tag));
        self.scroll_run_to_bottom();
    }

    pub fn clear_run(&self) {
        self.run_buf.set_text("");
        self.errors.borrow_mut().clear();
    }

    pub fn append_output(&self, text: &str) {
        append_line(&self.out_buf, text, None::<&TextTag>);
    }

    pub fn clear_output(&self) {
        self.out_buf.set_text("");
    }

    pub fn set_problems(&self, text: &str) {
        self.prob_buf.set_text(text);
    }

    pub fn clear_problems(&self) {
        self.prob_buf.set_text("");
    }

    /// Populate the Problems tab from a list of parsed diagnostics.
    /// Switches the stack to the Problems view.
    pub fn set_diagnostics(&self, diags: &[Diagnostic]) {
        self.prob_buf.set_text("");
        if diags.is_empty() {
            append_line(&self.prob_buf, "✓ No errors or warnings.", None::<&TextTag>);
            self.stack.set_visible_child_name("problems");
            return;
        }
        for d in diags {
            let (prefix, tag) = match d.level {
                DiagLevel::Error   => ("ERROR",   Some(&self.prob_error_tag)),
                DiagLevel::Warning => ("WARNING", Some(&self.prob_warning_tag)),
                DiagLevel::Note    => ("NOTE",    None),
            };
            append_line(&self.prob_buf, &format!("{}: {}", prefix, d.message), tag.map(|t| t as &TextTag));
            let file_name = d.file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| d.file.to_string_lossy().into_owned());
            let loc = format!("  --> {}:{}:{}", file_name, d.line_start, d.col_start);
            append_line(&self.prob_buf, &loc, Some(&self.prob_link_tag as &TextTag));
        }
        self.stack.set_visible_child_name("problems");
        self.widget.set_visible(true);
    }

    /// Set the callback invoked when the user submits a search query.
    pub fn set_on_search_request<F: Fn(&str) + 'static>(&self, f: F) {
        *self.on_search_request.borrow_mut() = Some(Box::new(f));
    }

    /// Populate the Search tab with ripgrep results.
    pub fn set_search_results(&self, query: &str, matches: &[SearchMatch]) {
        self.search_buf.set_text("");
        if matches.is_empty() {
            append_line(&self.search_buf, &format!("No results for «{}»", query), None::<&TextTag>);
            self.stack.set_visible_child_name("search");
            self.widget.set_visible(true);
            return;
        }

        // Group by file
        let mut current_file: Option<&PathBuf> = None;
        let mut file_count = 0usize;
        for m in matches {
            if current_file != Some(&m.file) {
                // Count matches for this file
                let n = matches.iter().filter(|x| x.file == m.file).count();
                let header = format!("── {} ({} match{}) ──",
                    m.file.display(), n, if n == 1 { "" } else { "es" });
                append_line(&self.search_buf, &header, Some(&self.search_file_tag as &TextTag));
                current_file = Some(&m.file);
                file_count += 1;
            }
            // Clickable location line
            let loc = format!("  --> {}:{}:{}", m.file.display(), m.line, m.col);
            append_line(&self.search_buf, &loc, Some(&self.search_link_tag as &TextTag));
            // Match text (plain, indented)
            let trimmed = m.text.trim_end_matches('\n');
            append_line(&self.search_buf, &format!("      {}", trimmed), None::<&TextTag>);
        }

        let summary = format!("\n{} match{} in {} file{}",
            matches.len(), if matches.len() == 1 { "" } else { "es" },
            file_count,   if file_count   == 1 { "" } else { "s" });
        append_line(&self.search_buf, &summary, None::<&TextTag>);

        self.stack.set_visible_child_name("search");
        self.widget.set_visible(true);
    }

    /// Show output panel on the Search tab and focus the query entry.
    pub fn focus_search(&self) {
        self.widget.set_visible(true);
        self.stack.set_visible_child_name("search");
        self.search_entry.grab_focus();
    }

    pub fn show_panel(&self) {
        self.widget.set_visible(true);
    }

    pub fn hide_panel(&self) {
        self.widget.set_visible(false);
    }

    pub fn toggle(&self) {
        self.widget.set_visible(!self.widget.is_visible());
    }

    pub fn switch_to_run(&self) {
        self.stack.set_visible_child_name("run");
    }

    fn scroll_run_to_bottom(&self) {
        let end = self.run_buf.end_iter();
        let mark = self.run_buf.create_mark(None, &end, false);
        // Do not place cursor! Scroll to mark instead.
        self.run_view.scroll_to_mark(&mark, 0.0, true, 0.0, 1.0);
    }
}

/// Like wire_click_handler but uses a custom parser function.
fn wire_click_handler_custom(
    view: &TextView,
    buf: &TextBuffer,
    tag_name: &'static str,
    cb: &ErrorClickCb,
    parser: fn(&str) -> Option<(PathBuf, i32, i32)>,
) {
    let rv  = view.clone();
    let buf = buf.clone();
    let cb  = cb.clone();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(1);
    gesture.connect_released(move |_, _, x, y| {
        let (bx, by) = rv.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
        if let Some(iter) = rv.iter_at_location(bx, by) {
            if let Some(tag) = buf.tag_table().lookup(tag_name) {
                if iter.has_tag(&tag) {
                    let mut ls = iter; ls.set_line_offset(0);
                    let mut le = iter;
                    if !le.ends_line() { le.forward_to_line_end(); }
                    let text = buf.text(&ls, &le, false).to_string();
                    if let Some((path, line, col)) = parser(&text) {
                        if let Some(ref f) = *cb.borrow() { f(path, line, col); }
                    }
                }
            }
        }
    });
    view.add_controller(gesture);
}

fn wire_click_handler(view: &TextView, buf: &TextBuffer, tag_name: &'static str, cb: &ErrorClickCb) {
    let rv = view.clone();
    let buf = buf.clone();
    let cb = cb.clone();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(1);
    gesture.connect_released(move |_, _, x, y| {
        let (bx, by) = rv.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
        if let Some(iter) = rv.iter_at_location(bx, by) {
            if let Some(tag) = buf.tag_table().lookup(tag_name) {
                if iter.has_tag(&tag) {
                    let mut ls = iter; ls.set_line_offset(0);
                    let mut le = iter;
                    if !le.ends_line() { le.forward_to_line_end(); }
                    let text = buf.text(&ls, &le, false).to_string();
                    if let Some((path, line, col)) = parse_error_location(&text) {
                        if let Some(ref f) = *cb.borrow() { f(path, line, col); }
                    }
                }
            }
        }
    });
    view.add_controller(gesture);
}

fn make_text_view() -> (TextView, TextBuffer) {
    let buf = TextBuffer::new(None);
    let view = TextView::builder()
        .buffer(&buf)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk4::WrapMode::Word)
        .hexpand(true)
        .vexpand(true)
        .build();
    view.add_css_class("editor-output");
    (view, buf)
}

fn wrap_scroll(view: TextView) -> ScrolledWindow {
    ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(100)
        .build()
}

fn append_line(buf: &TextBuffer, text: &str, tag: Option<&TextTag>) {
    let mut end = buf.end_iter();
    let is_empty = buf.start_iter() == end;
    let line = if is_empty {
        text.to_string()
    } else {
        format!("\n{}", text)
    };
    if let Some(t) = tag {
        buf.insert_with_tags(&mut end, &line, &[t]);
    } else {
        buf.insert(&mut end, &line);
    }
}

/// Check if a line looks like a cargo error location: ` --> file:line:col`
fn is_error_location(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("--> ")
}

/// Parse `  --> /abs/path/file.rs:42:5` (search results format) into (PathBuf, line, col).
fn parse_search_location(text: &str) -> Option<(PathBuf, i32, i32)> {
    parse_error_location(text)
}

/// Parse ` --> path/file.rs:42:5` into (PathBuf, line, col).
fn parse_error_location(text: &str) -> Option<(PathBuf, i32, i32)> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix("--> ")?;
    // rest = "src/main.rs:15:9" or "src/main.rs:15:9: note"
    let parts: Vec<&str> = rest.splitn(4, ':').collect();
    if parts.len() >= 3 {
        let file = PathBuf::from(parts[0]);
        let line = parts[1].parse::<i32>().ok()?;
        let col = parts[2].trim_end().parse::<i32>().ok().unwrap_or(1);
        Some((file, line, col))
    } else {
        None
    }
}
