use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Notebook, Orientation, ScrolledWindow, TextBuffer, TextTag,
    TextView,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;

/// Callback invoked when the user clicks an error location in the Run output.
pub type ErrorClickCb = Rc<RefCell<Option<Box<dyn Fn(PathBuf, i32, i32)>>>>;
pub type AskAiCb = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;

#[derive(Clone)]
pub struct OutputPanel {
    pub widget:  GtkBox,
    stack:       gtk4::Stack,
    run_buf:     TextBuffer,
    run_view:    TextView,
    out_buf:     TextBuffer,
    prob_buf:    TextBuffer,
    error_tag:   TextTag,
    success_tag: TextTag,
    link_tag:    TextTag,
    /// Called with (file_path, line, col) when user clicks an error location.
    on_error_click: ErrorClickCb,
    /// Called with the extracted error text when the robot button is clicked.
    on_ask_ai: AskAiCb,
    /// Accumulated error lines (populated by append_run_error).
    errors: Rc<RefCell<Vec<String>>>,
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

        stack.add_titled(&wrap_scroll(out_view.clone()), Some("output"), "Output");
        stack.add_titled(&wrap_scroll(prob_view), Some("problems"), "Problems");
        stack.add_titled(&wrap_scroll(run_view.clone()), Some("run"), "Run");

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
        {
            let rv = run_view.clone();
            let buf = run_buf.clone();
            let cb = on_error_click.clone();
            let gesture = gtk4::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_released(move |_, _, x, y| {
                let (bx, by) = rv.window_to_buffer_coords(
                    gtk4::TextWindowType::Widget,
                    x as i32,
                    y as i32,
                );
                if let Some(iter) = rv.iter_at_location(bx, by) {
                    if let Some(tag) = buf.tag_table().lookup("link") {
                        if iter.has_tag(&tag) {
                            let mut line_start = iter;
                            line_start.set_line_offset(0);
                            let mut line_end = iter;
                            if !line_end.ends_line() {
                                line_end.forward_to_line_end();
                            }
                            let line_text = buf.text(&line_start, &line_end, false).to_string();
                            if let Some((path, line, col)) = parse_error_location(&line_text) {
                                if let Some(ref f) = *cb.borrow() {
                                    f(path, line, col);
                                }
                            }
                        }
                    }
                }
            });
            run_view.add_controller(gesture);
        }

        Self {
            widget: vbox,
            stack,
            run_view,
            run_buf,
            out_buf,
            prob_buf,
            error_tag,
            success_tag,
            link_tag,
            on_error_click,
            on_ask_ai,
            errors,
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
