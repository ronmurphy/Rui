//! AI Chat sidebar — embedded right-hand panel with persistent WebKit sessions.
//!
//! Shows/hides as a sidebar inside the main Rui window.  When shown the window
//! is widened by SIDEBAR_WIDTH so the designer area keeps its full size.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, Label, Orientation,
};
use webkit6::prelude::*;
use webkit6::{WebView, NetworkSession, Settings};
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

pub const SIDEBAR_WIDTH: i32 = 450;

// Firefox 120 on Linux — widely accepted by Claude, ChatGPT, Gemini, etc.
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0";

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

    let settings = Settings::new();
    settings.set_user_agent(Some(USER_AGENT));

    WebView::builder()
        .network_session(&network_session)
        .settings(&settings)
        .build()
}

/// Embeddable AI sidebar.  Add `sidebar.widget` to your layout.
#[derive(Clone)]
pub struct AiSidebar {
    pub widget: GtkBox,
}

impl AiSidebar {
    pub fn new() -> Self {
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(SIDEBAR_WIDTH);
        widget.set_hexpand(false);
        widget.set_vexpand(true);
        widget.set_visible(false); // hidden until toggled

        // ── Toolbar ───────────────────────────────────────────────
        let toolbar = GtkBox::new(Orientation::Horizontal, 4);
        toolbar.set_margin_start(6);
        toolbar.set_margin_end(6);
        toolbar.set_margin_top(4);
        toolbar.set_margin_bottom(2);

        let service_lbl = Label::new(Some("Open:"));
        service_lbl.set_margin_end(2);
        toolbar.append(&service_lbl);

        let url_entry = Entry::new();
        url_entry.set_hexpand(true);
        url_entry.set_placeholder_text(Some("URL"));
        url_entry.add_css_class("monospace");

        let go_btn = Button::with_label("Go");

        let initial_service = &SERVICES[0];
        let wv_cell = Rc::new(RefCell::new(
            make_web_view(session_base().join(initial_service.dir))
        ));
        wv_cell.borrow().set_hexpand(true);
        wv_cell.borrow().set_vexpand(true);

        let wv_container = GtkBox::new(Orientation::Vertical, 0);
        wv_container.set_hexpand(true);
        wv_container.set_vexpand(true);
        wv_container.append(&*wv_cell.borrow());

        wv_cell.borrow().load_uri(initial_service.url);
        url_entry.set_text(initial_service.url);

        // Service buttons
        for svc in SERVICES {
            let btn = Button::with_label(svc.label);
            btn.set_tooltip_text(Some(svc.url));

            let wvc     = wv_cell.clone();
            let cont    = wv_container.clone();
            let url_e   = url_entry.clone();
            let svc_dir = session_base().join(svc.dir);
            let svc_url = svc.url;

            btn.connect_clicked(move |_| {
                let old = wvc.borrow().clone();
                cont.remove(&old);
                let nw = make_web_view(svc_dir.clone());
                nw.set_hexpand(true);
                nw.set_vexpand(true);
                nw.load_uri(svc_url);
                cont.append(&nw);
                *wvc.borrow_mut() = nw;
                url_e.set_text(svc_url);
            });

            toolbar.append(&btn);
        }

        // URL bar
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        toolbar.append(&spacer);
        toolbar.append(&url_entry);
        toolbar.append(&go_btn);

        {
            let wvc = wv_cell.clone();
            let e   = url_entry.clone();
            go_btn.connect_clicked(move |_| {
                let mut url = e.text().to_string();
                if !url.contains("://") { url = format!("https://{}", url); }
                wvc.borrow().load_uri(&url);
            });
        }
        {
            let wvc = wv_cell.clone();
            url_entry.connect_activate(move |e| {
                let mut url = e.text().to_string();
                if !url.contains("://") { url = format!("https://{}", url); }
                wvc.borrow().load_uri(&url);
            });
        }
        {
            let e = url_entry.clone();
            wv_cell.borrow().connect_load_changed(move |wv, _| {
                if let Some(uri) = wv.uri() { e.set_text(&uri); }
            });
        }

        widget.append(&toolbar);
        widget.append(&wv_container);

        AiSidebar { widget }
    }

    /// Show or hide the sidebar, returning true if now visible.
    pub fn toggle(&self) -> bool {
        let visible = !self.widget.is_visible();
        self.widget.set_visible(visible);
        visible
    }

    pub fn is_visible(&self) -> bool {
        self.widget.is_visible()
    }
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
