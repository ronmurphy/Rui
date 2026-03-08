use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation};
use std::path::Path;
use webkit6::prelude::*;
use webkit6::WebView;

#[derive(Clone)]
pub struct PreviewPane {
    pub widget: GtkBox,
    web_view:   WebView,
    visible:    std::rc::Rc<std::cell::RefCell<bool>>,
}

impl PreviewPane {
    pub fn new() -> Self {
        let vbox = GtkBox::new(Orientation::Vertical, 0);
        vbox.add_css_class("editor-preview-bar");

        let header = Label::new(Some("Preview"));
        header.set_xalign(0.0);
        header.set_margin_start(8);
        header.set_margin_top(4);
        header.set_margin_bottom(4);
        header.add_css_class("editor-statusbar-item");

        let web_view = WebView::new();
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);

        vbox.append(&header);
        vbox.append(&web_view);

        let visible = std::rc::Rc::new(std::cell::RefCell::new(true));

        Self { widget: vbox, web_view, visible }
    }

    pub fn load_html_file(&self, path: &Path) {
        if let Ok(content) = std::fs::read_to_string(path) {
            let base_uri = path
                .parent()
                .map(|p| format!("file://{}/", p.display()))
                .unwrap_or_else(|| "file:///".to_string());
            self.web_view.load_html(&content, Some(&base_uri));
        }
    }

    pub fn load_css_file(&self, path: &Path) {
        if let Ok(css) = std::fs::read_to_string(path) {
            let html = format!(
                r#"<!DOCTYPE html><html><head>
<meta charset="utf-8">
<style>{css}</style>
</head><body>
<h1>CSS Preview</h1>
<p>This is a paragraph styled by your CSS.</p>
<button>A Button</button>
<ul><li>Item one</li><li>Item two</li></ul>
</body></html>"#
            );
            self.web_view.load_html(&html, None);
        }
    }

    pub fn reload_from_file(&self, path: &Path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "html" | "htm" => self.load_html_file(path),
            "css"          => self.load_css_file(path),
            _ => {}
        }
    }

    pub fn load_html_string(&self, html: &str, base_uri: Option<&str>) {
        self.web_view.load_html(html, base_uri);
    }

    pub fn show(&self) {
        self.widget.set_visible(true);
        *self.visible.borrow_mut() = true;
    }

    pub fn hide(&self) {
        self.widget.set_visible(false);
        *self.visible.borrow_mut() = false;
    }

    pub fn toggle(&self) {
        if *self.visible.borrow() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn is_visible_pref(&self) -> bool {
        *self.visible.borrow()
    }
}
