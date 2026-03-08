use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Notebook, Orientation, ScrolledWindow, TextBuffer, TextTag,
    TextView,
};

#[derive(Clone)]
pub struct OutputPanel {
    pub widget:  GtkBox,
    notebook:    Notebook,
    run_buf:     TextBuffer,
    out_buf:     TextBuffer,
    prob_buf:    TextBuffer,
    error_tag:   TextTag,
    success_tag: TextTag,
}

impl OutputPanel {
    pub fn new() -> Self {
        let vbox = GtkBox::new(Orientation::Vertical, 0);

        let notebook = Notebook::builder()
            .scrollable(true)
            .show_border(false)
            .build();

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
        run_buf.tag_table().add(&error_tag);
        run_buf.tag_table().add(&success_tag);

        notebook.append_page(
            &wrap_scroll(out_view),
            Some(&gtk4::Label::new(Some("Output"))),
        );
        notebook.append_page(
            &wrap_scroll(prob_view),
            Some(&gtk4::Label::new(Some("Problems"))),
        );
        notebook.append_page(
            &wrap_scroll(run_view),
            Some(&gtk4::Label::new(Some("Run"))),
        );

        vbox.append(&notebook);

        Self {
            widget: vbox,
            notebook,
            run_buf,
            out_buf,
            prob_buf,
            error_tag,
            success_tag,
        }
    }

    pub fn append_run_line(&self, text: &str) {
        append_line(&self.run_buf, text, None::<&TextTag>);
        self.scroll_run_to_bottom();
        self.notebook.set_current_page(Some(2));
    }

    pub fn append_run_error(&self, text: &str) {
        append_line(&self.run_buf, text, Some(&self.error_tag));
        self.scroll_run_to_bottom();
        self.notebook.set_current_page(Some(2));
    }

    pub fn append_run_success(&self, text: &str) {
        append_line(&self.run_buf, text, Some(&self.success_tag));
        self.scroll_run_to_bottom();
    }

    pub fn clear_run(&self) {
        self.run_buf.set_text("");
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
        self.notebook.set_current_page(Some(2));
    }

    fn scroll_run_to_bottom(&self) {
        let end = self.run_buf.end_iter();
        let mark = self.run_buf.create_mark(None, &end, false);
        self.run_buf.place_cursor(&end);
        let _ = mark;
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
