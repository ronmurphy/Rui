//! AI Chat panel — a dedicated WebView window with persistent session storage.
//!
//! Each AI service gets its own persistent cookie/localStorage profile stored in
//! ~/.local/share/rui/ai-session/<service>/

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, Label, Orientation, Window,
};
use webkit6::prelude::*;
use webkit6::{WebView, NetworkSession};
use std::path::PathBuf;

struct AiService {
    label: &'static str,
    url:   &'static str,
    dir:   &'static str,
}

const SERVICES: &[AiService] = &[
    AiService { label: "Claude",  url: "https://claude.ai",          dir: "claude"  },
    AiService { label: "ChatGPT", url: "https://chatgpt.com",        dir: "chatgpt" },
    AiService { label: "Gemini",  url: "https://gemini.google.com",  dir: "gemini"  },
    AiService { label: "Codex",   url: "https://github.com/copilot", dir: "codex"   },
];

fn session_base() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("rui")
        .join("ai-session")
}

fn make_web_view(session_dir: PathBuf) -> WebView {
    let _ = std::fs::create_dir_all(&session_dir);

    let data_dir  = session_dir.to_string_lossy().to_string();
    let cache_dir = session_dir.join("cache").to_string_lossy().to_string();

    let network_session = NetworkSession::new(Some(&data_dir), Some(&cache_dir));
    WebView::builder()
        .network_session(&network_session)
        .build()
}

pub fn show_ai_panel(parent: &gtk4::ApplicationWindow) {
    let win = Window::builder()
        .title("AI Chat — Rui")
        .transient_for(parent)
        .modal(false)
        .default_width(900)
        .default_height(720)
        .resizable(true)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);

    let toolbar = GtkBox::new(Orientation::Horizontal, 4);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);
    toolbar.set_margin_top(6);
    toolbar.set_margin_bottom(4);

    let service_lbl = Label::new(Some("Open:"));
    service_lbl.set_margin_end(4);
    toolbar.append(&service_lbl);

    let url_entry = Entry::new();
    url_entry.set_hexpand(true);
    url_entry.set_placeholder_text(Some("URL"));
    url_entry.add_css_class("monospace");

    let go_btn = Button::with_label("Go");

    let initial_service = &SERVICES[0];
    let web_view_cell = std::rc::Rc::new(std::cell::RefCell::new(
        make_web_view(session_base().join(initial_service.dir))
    ));
    web_view_cell.borrow().set_hexpand(true);
    web_view_cell.borrow().set_vexpand(true);

    let wv_container = GtkBox::new(Orientation::Vertical, 0);
    wv_container.set_hexpand(true);
    wv_container.set_vexpand(true);
    wv_container.append(&*web_view_cell.borrow());

    web_view_cell.borrow().load_uri(initial_service.url);
    url_entry.set_text(initial_service.url);

    for svc in SERVICES {
        let btn = Button::with_label(svc.label);
        btn.set_tooltip_text(Some(svc.url));

        let wv_cell   = web_view_cell.clone();
        let container = wv_container.clone();
        let url_e     = url_entry.clone();
        let svc_dir   = session_base().join(svc.dir);
        let svc_url   = svc.url;

        btn.connect_clicked(move |_| {
            let old = wv_cell.borrow().clone();
            container.remove(&old);
            let new_wv = make_web_view(svc_dir.clone());
            new_wv.set_hexpand(true);
            new_wv.set_vexpand(true);
            new_wv.load_uri(svc_url);
            container.append(&new_wv);
            *wv_cell.borrow_mut() = new_wv;
            url_e.set_text(svc_url);
        });

        toolbar.append(&btn);
    }

    let sep_box = GtkBox::new(Orientation::Horizontal, 0);
    sep_box.set_hexpand(true);
    toolbar.append(&sep_box);
    toolbar.append(&url_entry);
    toolbar.append(&go_btn);

    {
        let wv = web_view_cell.clone();
        let entry = url_entry.clone();
        go_btn.connect_clicked(move |_| {
            let mut url = entry.text().to_string();
            if !url.contains("://") { url = format!("https://{}", url); }
            wv.borrow().load_uri(&url);
        });
    }
    {
        let wv = web_view_cell.clone();
        url_entry.connect_activate(move |e| {
            let mut url = e.text().to_string();
            if !url.contains("://") { url = format!("https://{}", url); }
            wv.borrow().load_uri(&url);
        });
    }

    {
        let entry = url_entry.clone();
        web_view_cell.borrow().connect_load_changed(move |wv, _| {
            if let Some(uri) = wv.uri() {
                entry.set_text(&uri);
            }
        });
    }

    root.append(&toolbar);
    root.append(&wv_container);
    win.set_child(Some(&root));
    win.present();
}

pub fn format_for_ai(filename: &str, lang: &str, code: &str, question: &str) -> String {
    let question_block = if question.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n\n", question.trim())
    };

    format!(
        "{question_block}\
File: `{filename}`\n\
\n\
```{lang}\n\
{code}\n\
```\n\
\n\
Please respond **only** with a unified git diff in this format:\n\
\n\
```diff\n\
--- a/{filename}\n\
+++ b/{filename}\n\
@@ -line,count +line,count @@\n\
 context line\n\
-removed line\n\
+added line\n\
```\n\
\n\
Do not include any explanation outside the diff block.",
        question_block = question_block,
        filename = filename,
        lang = lang,
        code = code,
    )
}
