//! ai_chat_panel.rs — Native multi-provider AI chat sidebar.
//!
//! Provider-agnostic chat panel that talks directly to REST APIs
//! (OpenAI, Anthropic, Gemini, any OpenAI-compat endpoint) using API keys
//! configured via the built-in ⚙ settings popover.  No external CLI required.
//!
//! The panel follows the same window-widening contract as `claude_code.rs`:
//!   - SIDEBAR_WIDTH  — caller adds/subtracts this from the window width on toggle
//!   - toggle() → bool — returns whether the panel is now visible
//!   - kill_process() — no-op here but kept for API symmetry

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Entry, Label, Orientation,
    Popover, ScrolledWindow, Separator, Spinner, StringList, TextView, WrapMode,
    InputHints, InputPurpose,
};
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::time::Duration;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::ai_provider::{AiConfig, AiProvider, StreamEvent};

pub const SIDEBAR_WIDTH: i32 = 450;

// const SYSTEM_PROMPT: &str = r#"You are a GTK4 UI design assistant embedded in "Rui", a visual .ui file designer for Rust developers.

// The user's project has two key files:
// 1. A .ui file (GTK4 Builder XML) — the layout
// 2. A companion .rs file (Rust code using gtk4-rs 0.9) — signal handlers and app logic

// When you respond:
// - Wrap UI XML changes in ```xml code blocks
// - Wrap Rust code changes in ```rust code blocks
// - The user can apply each block independently to the correct file
// - Keep responses concise and code-focused."#;

const SYSTEM_PROMPT: &str = r#"You are a GTK4 UI design assistant embedded in "Rui", a visual .ui file designer for Rust developers.

The user's project has two key files:
1. A .ui file (GTK4 Builder XML) — the layout
2. A companion .rs file (Rust code using gtk4-rs 0.9) — signal handlers and app logic

Key facts about Rui's XML format:
- Files are standard GTK4 Builder XML with root <interface> containing <object> elements
- Widgets are wrapped in <child> blocks: <child><object class="GtkButton">...</object></child>
- GtkGrid children use <layout> inside <object> for positioning:
  <layout><property name="column">0</property><property name="row">0</property>
  <property name="column-span">1</property><property name="row-span">1</property></layout>
- Custom design-time properties: rui-rows, rui-columns (grid dimensions, stripped at runtime)
- Common containers: GtkBox, GtkGrid, GtkFrame, GtkPaned, GtkNotebook, GtkStack
- Common widgets: GtkButton, GtkLabel, GtkEntry, GtkToggleButton, GtkScale, GtkSwitch, GtkSpinner
- Give each widget an `id` attribute so the Rust code can look it up from the Builder

Key facts about the companion Rust code:
- Uses gtk4-rs 0.9 with the v4_10 feature
- Loads the .ui file with gtk4::Builder::from_file() or from_string()
- Looks up widgets by id: builder.object::<Button>("my_button").unwrap()
- Connects signals: button.connect_clicked(|_| { ... })
- Uses gtk4::prelude::* for trait methods

When you respond:
- For NEW code or full rewrites: wrap in ```xml or ```rust code blocks
- For FIXING compile errors or small edits: respond with a unified git diff in a ```diff code block
  The diff format must be a valid unified diff:
  ```diff
  --- a/src/layout_app.rs
  +++ b/src/layout_app.rs
  @@ -10,6 +10,7 @@
   context line
  -old line
  +new line
  ```
- The user can apply each block independently to the correct file
- Keep responses concise and code-focused."#;


// ── Chat message record ───────────────────────────────────────────────────────

#[derive(Clone)]
struct ChatMessage {
    role:    String,
    content: String,
}

// ── Code block helpers (same as claude_code.rs) ───────────────────────────────

struct CodeBlock {
    lang: String,
    code: String,
}

fn extract_code_blocks(text: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    let mut lines  = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") && trimmed.len() > 3 {
            let lang = trimmed[3..].trim().to_string();
            let mut code = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" { break; }
                if !code.is_empty() { code.push('\n'); }
                code.push_str(inner);
            }
            if !code.is_empty() {
                blocks.push(CodeBlock { lang, code });
            }
        }
    }
    blocks
}

/// Extract a filename from a unified diff's `---` or `+++` header lines.
fn extract_diff_filename(diff: &str) -> Option<String> {
    for line in diff.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("+++ ") {
            let path = trimmed.strip_prefix("+++ ").unwrap().trim();
            if path == "/dev/null" { continue; }
            let clean = path.strip_prefix("b/")
                .or_else(|| path.strip_prefix("a/"))
                .unwrap_or(path);
            let name = std::path::Path::new(clean)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(clean);
            return Some(name.to_string());
        }
    }
    for line in diff.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--- ") {
            let path = trimmed.strip_prefix("--- ").unwrap().trim();
            if path == "/dev/null" { continue; }
            let clean = path.strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path);
            let name = std::path::Path::new(clean)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(clean);
            return Some(name.to_string());
        }
    }
    None
}

/// Try to extract a filename from code block content.
fn extract_code_filename(code: &str, lang: &str) -> Option<String> {
    for line in code.lines().take(5) {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("// file:") || lower.starts_with("// filename:") {
            let rest = trimmed.splitn(2, ':').nth(1)?.trim();
            let name = std::path::Path::new(rest)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(rest);
            return Some(name.to_string());
        }
        if lower.starts_with("<!-- file:") || lower.starts_with("<!-- filename:") {
            let rest = trimmed.splitn(2, ':').nth(1)?.trim();
            let name = rest.trim_end_matches("-->").trim();
            let name = std::path::Path::new(name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(name);
            return Some(name.to_string());
        }
    }
    _ = lang;
    None
}

// ── Panel struct ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AiChatPanel {
    pub widget: GtkBox,
    messages:   Rc<RefCell<Vec<ChatMessage>>>,
    chat_box:   GtkBox,
    input:      TextView,
    send_btn:   Button,
    scroll:     ScrolledWindow,

    config: Rc<RefCell<AiConfig>>,

    get_buffer_cb:    Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>>,
    get_path_cb:      Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>>,
    get_companion_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<(String, String)>>>>>,
    on_apply_xml_cb:  Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_apply_rs_cb:   Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_apply_diff_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,

    busy:       Rc<RefCell<bool>>,
    spinner:    Spinner,
    child_proc: Arc<Mutex<Option<u32>>>,  // unused, kept for API parity
}

impl AiChatPanel {
    pub fn new() -> Self {
        let cfg = crate::config::load_ai_config();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(SIDEBAR_WIDTH);
        widget.set_hexpand(false);
        widget.set_vexpand(true);
        widget.set_visible(false);

        // ── Header ──────────────────────────────────────────────────
        let header_box = GtkBox::new(Orientation::Horizontal, 6);
        header_box.set_margin_start(8);
        header_box.set_margin_end(8);
        header_box.set_margin_top(6);
        header_box.set_margin_bottom(4);

        let title = Label::new(Some("AI Chat"));
        title.add_css_class("heading");
        title.set_hexpand(true);
        title.set_halign(gtk4::Align::Start);

        let clear_btn = Button::with_label("Clear");
        clear_btn.add_css_class("flat");
        clear_btn.set_tooltip_text(Some("Clear chat history"));

        // Gear button → settings popover (nf-fa-gear via FiraCode Nerd Font)
        let gear_btn = Button::with_label("\u{F013}");
        gear_btn.add_css_class("flat");
        gear_btn.add_css_class("nf");
        gear_btn.set_tooltip_text(Some("Configure AI provider & API key"));

        header_box.append(&title);
        header_box.append(&clear_btn);
        header_box.append(&gear_btn);

        // ── Chat history ─────────────────────────────────────────────
        let chat_box = GtkBox::new(Orientation::Vertical, 8);
        chat_box.set_margin_start(8);
        chat_box.set_margin_end(8);
        chat_box.set_margin_top(4);
        chat_box.set_margin_bottom(4);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&chat_box)
            .build();

        // ── Input area ───────────────────────────────────────────────
        let input_box = GtkBox::new(Orientation::Vertical, 4);
        input_box.set_margin_start(8);
        input_box.set_margin_end(8);
        input_box.set_margin_top(4);
        input_box.set_margin_bottom(8);

        let input = TextView::new();
        input.set_wrap_mode(WrapMode::WordChar);
        input.set_height_request(60);
        input.add_css_class("monospace");
        input.set_top_margin(4);
        input.set_bottom_margin(4);
        input.set_left_margin(6);
        input.set_right_margin(6);

        let input_scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(false)
            .max_content_height(120)
            .child(&input)
            .build();

        let btn_row = GtkBox::new(Orientation::Horizontal, 6);
        let ctx_lbl = Label::new(Some("Includes .ui + companion .rs context"));
        ctx_lbl.add_css_class("dim-label");
        ctx_lbl.set_halign(gtk4::Align::Start);
        ctx_lbl.set_hexpand(true);

        let send_btn = Button::with_label("Send");
        send_btn.add_css_class("suggested-action");
        send_btn.set_tooltip_text(Some("Send (Ctrl+Enter)"));

        let spinner = Spinner::new();
        spinner.set_visible(false);

        btn_row.append(&ctx_lbl);
        btn_row.append(&spinner);
        btn_row.append(&send_btn);
        input_box.append(&input_scroll);
        input_box.append(&btn_row);

        // ── Assemble ─────────────────────────────────────────────────
        widget.append(&header_box);
        widget.append(&Separator::new(Orientation::Horizontal));
        widget.append(&scroll);
        widget.append(&Separator::new(Orientation::Horizontal));
        widget.append(&input_box);

        let config = Rc::new(RefCell::new(cfg));

        let panel = AiChatPanel {
            widget,
            messages:         Rc::new(RefCell::new(Vec::new())),
            chat_box,
            input,
            send_btn: send_btn.clone(),
            scroll,
            config:           config.clone(),
            get_buffer_cb:    Rc::new(RefCell::new(None)),
            get_path_cb:      Rc::new(RefCell::new(None)),
            get_companion_cb: Rc::new(RefCell::new(None)),
            on_apply_xml_cb:  Rc::new(RefCell::new(None)),
            on_apply_rs_cb:   Rc::new(RefCell::new(None)),
            on_apply_diff_cb: Rc::new(RefCell::new(None)),
            busy:             Rc::new(RefCell::new(false)),
            spinner,
            child_proc:       Arc::new(Mutex::new(None)),
        };

        // Wire Send button
        { let p = panel.clone(); send_btn.connect_clicked(move |_| p.do_send()); }

        // Wire Ctrl+Enter
        {
            let p = panel.clone();
            let kc = gtk4::EventControllerKey::new();
            kc.connect_key_pressed(move |_, key, _, mods| {
                if key == gtk4::gdk::Key::Return
                    && mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                {
                    p.do_send();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            panel.input.add_controller(kc);
        }

        // Wire Clear button
        { let p = panel.clone(); clear_btn.connect_clicked(move |_| p.clear_chat()); }

        // Wire Gear button → settings popover
        {
            let p = panel.clone();
            gear_btn.connect_clicked(move |btn| {
                p.show_settings_popover(btn);
            });
        }

        panel
    }

    // ── Callback registration ─────────────────────────────────────────────────

    pub fn on_get_buffer<F: Fn() -> Option<String> + 'static>(&self, cb: F) {
        *self.get_buffer_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_get_path<F: Fn() -> Option<String> + 'static>(&self, cb: F) {
        *self.get_path_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_get_companion<F: Fn() -> Option<(String, String)> + 'static>(&self, cb: F) {
        *self.get_companion_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_xml<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_xml_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_rs<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_rs_cb.borrow_mut() = Some(Box::new(cb));
    }
    pub fn on_apply_diff<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_diff_cb.borrow_mut() = Some(Box::new(cb));
    }

    // ── Toggle / visibility ───────────────────────────────────────────────────

    /// Populate the input text box (used by the robot button in the output panel).
    pub fn set_input(&self, text: &str) {
        self.input.buffer().set_text(text);
        self.input.grab_focus();
    }

    pub fn toggle(&self) -> bool {
        let visible = !self.widget.is_visible();
        self.widget.set_visible(visible);
        if visible { self.input.grab_focus(); }
        visible
    }

    pub fn is_visible(&self) -> bool { self.widget.is_visible() }

    /// No-op — kept for API parity with ClaudeCodePanel.
    pub fn kill_process(&self) {}

    // ── Settings popover ──────────────────────────────────────────────────────

    fn show_settings_popover(&self, anchor: &Button) {
        let cfg = self.config.borrow().clone();

        let pop = Popover::new();
        pop.set_parent(anchor);

        let vbox = GtkBox::new(Orientation::Vertical, 8);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);
        vbox.set_margin_top(12);
        vbox.set_margin_bottom(12);

        // Provider dropdown
        let prov_lbl = Label::new(Some("Provider"));
        prov_lbl.set_halign(gtk4::Align::Start);
        let labels: Vec<&str> = AiProvider::all().iter().map(|p| p.label()).collect();
        let sl = StringList::new(&labels);
        let provider_dd = DropDown::new(Some(sl), gtk4::Expression::NONE);
        let cur_idx = AiProvider::all()
            .iter()
            .position(|p| p == &cfg.provider)
            .unwrap_or(0) as u32;
        provider_dd.set_selected(cur_idx);

        // Model entry
        let model_lbl = Label::new(Some("Model"));
        model_lbl.set_halign(gtk4::Align::Start);
        let model_entry = Entry::new();
        model_entry.set_placeholder_text(Some("e.g. gpt-4o"));
        model_entry.set_text(&cfg.model);

        // API key entry
        let key_lbl = Label::new(Some("API Key"));
        key_lbl.set_halign(gtk4::Align::Start);
        let key_entry = Entry::new();
        key_entry.set_visibility(false);
        key_entry.set_input_purpose(InputPurpose::Password);
        key_entry.set_input_hints(InputHints::NO_SPELLCHECK);
        key_entry.set_placeholder_text(Some("sk-… / AIza… / etc."));
        key_entry.set_text(&cfg.api_key);

        // Base URL (for OpenAI-compat)
        let url_lbl = Label::new(Some("Base URL (OpenAI-compat)"));
        url_lbl.set_halign(gtk4::Align::Start);
        let url_entry = Entry::new();
        url_entry.set_placeholder_text(Some("http://localhost:11434/v1"));
        url_entry.set_text(&cfg.base_url);

        // Fill default model when provider changes
        {
            let me = model_entry.clone();
            provider_dd.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                let default_model = AiProvider::all()
                    .get(idx)
                    .map(|p| p.default_model())
                    .unwrap_or("");
                if me.text().is_empty() {
                    me.set_text(default_model);
                }
            });
        }

        // Save button
        let save_btn = Button::with_label("Save");
        save_btn.add_css_class("suggested-action");

        let config_rc = self.config.clone();
        let pop2 = pop.clone();
        let key_entry_c      = key_entry.clone();
        let url_entry_c      = url_entry.clone();
        let model_entry_c    = model_entry.clone();
        let provider_dd_c    = provider_dd.clone();
        save_btn.connect_clicked(move |_| {
            let idx = provider_dd_c.selected() as usize;
            let provider = AiProvider::all()
                .get(idx)
                .cloned()
                .unwrap_or_default();
            let new_cfg = AiConfig {
                provider,
                api_key:  key_entry_c.text().to_string(),
                model:    model_entry_c.text().to_string(),
                base_url: url_entry_c.text().to_string(),
            };
            *config_rc.borrow_mut() = new_cfg.clone();
            crate::config::save_ai_config(&new_cfg);
            pop2.popdown();
        });

        vbox.append(&prov_lbl);
        vbox.append(&provider_dd);
        vbox.append(&model_lbl);
        vbox.append(&model_entry);
        vbox.append(&key_lbl);
        vbox.append(&key_entry);
        vbox.append(&url_lbl);
        vbox.append(&url_entry);
        vbox.append(&save_btn);

        pop.set_child(Some(&vbox));
        pop.popup();
    }

    // ── Chat logic ────────────────────────────────────────────────────────────

    fn clear_chat(&self) {
        self.messages.borrow_mut().clear();
        while let Some(child) = self.chat_box.first_child() {
            self.chat_box.remove(&child);
        }
    }

    fn do_send(&self) {
        if *self.busy.borrow() { return; }

        let buf  = self.input.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let text = text.trim().to_string();
        if text.is_empty() { return; }
        buf.set_text("");

        // Gather context
        let ui_content = self.get_buffer_cb.borrow().as_ref().and_then(|cb| cb());
        let file_path  = self.get_path_cb.borrow().as_ref().and_then(|cb| cb());
        let companion  = self.get_companion_cb.borrow().as_ref().and_then(|cb| cb());

        // Build user message with context prefix
        let mut prompt = String::new();
        if let Some(ref path) = file_path {
            prompt.push_str(&format!("[Current .ui file: {}]\n\n", path));
        }
        if let Some(ref ui) = ui_content {
            prompt.push_str("Current .ui file contents:\n```xml\n");
            prompt.push_str(ui);
            prompt.push_str("\n```\n\n");
        }
        if let Some((ref rs_path, ref rs_content)) = companion {
            prompt.push_str(&format!("[Companion Rust file: {}]\n\n", rs_path));
            prompt.push_str("Current companion .rs file contents:\n```rust\n");
            prompt.push_str(rs_content);
            prompt.push_str("\n```\n\n");
        }
        prompt.push_str(&text);

        self.add_message("user", &text);
        let (response_label, anim_stop) = self.add_streaming_placeholder();

        *self.busy.borrow_mut() = true;
        self.send_btn.set_visible(false);
        self.spinner.set_visible(true);
        self.spinner.start();

        // Build message history for API
        let mut api_messages: Vec<(String, String)> = self.messages
            .borrow()
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        // Replace last user content with the full context-enriched prompt
        if let Some(last) = api_messages.last_mut() {
            if last.0 == "user" {
                last.1 = prompt.clone();
            }
        }

        let cfg = self.config.borrow().clone();
        let (tx, rx) = async_channel::unbounded::<StreamEvent>();
        let proc_handle = self.child_proc.clone();

        std::thread::spawn(move || {
            let result = crate::ai_provider::stream_chat(
                &cfg,
                &api_messages,
                SYSTEM_PROMPT,
                &tx,
                &proc_handle,
            );
            if let Err(e) = result {
                let _ = tx.send_blocking(StreamEvent::Error(e));
            }
            let _ = tx.send_blocking(StreamEvent::Done);
        });

        let panel = self.clone();
        let accumulated = Rc::new(RefCell::new(String::new()));
        glib::idle_add_local(move || {
            while let Ok(event) = rx.try_recv() {
                match event {
                    StreamEvent::Text(chunk) => {
                        accumulated.borrow_mut().push_str(&chunk);
                        response_label.set_text(&accumulated.borrow());
                        panel.scroll_to_bottom();
                    }
                    StreamEvent::Error(msg) => {
                        let cur = accumulated.borrow().clone();
                        let err = if cur.is_empty() {
                            format!("[Error: {}]", msg)
                        } else {
                            format!("{}\n\n[Error: {}]", cur, msg)
                        };
                        response_label.set_text(&err);
                        response_label.add_css_class("error");
                    }
                    StreamEvent::Done => {
                        anim_stop.set(true);
                        *panel.busy.borrow_mut() = false;
                        panel.spinner.stop();
                        panel.spinner.set_visible(false);
                        panel.send_btn.set_visible(true);

                        let final_text = accumulated.borrow().clone();
                        if !final_text.is_empty() {
                            panel.messages.borrow_mut().push(ChatMessage {
                                role:    "assistant".into(),
                                content: final_text.clone(),
                            });
                            panel.add_apply_buttons_for(&final_text, &response_label);
                        }
                        panel.scroll_to_bottom();
                        return glib::ControlFlow::Break;
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    fn add_message(&self, role: &str, content: &str) {
        self.messages.borrow_mut().push(ChatMessage {
            role: role.into(), content: content.into(),
        });

        let msg_box = GtkBox::new(Orientation::Vertical, 2);
        msg_box.set_margin_top(4);
        msg_box.set_margin_bottom(4);

        let role_lbl = Label::new(Some(if role == "user" { "You" } else { "AI" }));
        role_lbl.set_halign(gtk4::Align::Start);
        role_lbl.add_css_class("heading");

        let text_lbl = Label::new(Some(content));
        text_lbl.set_halign(gtk4::Align::Start);
        text_lbl.set_wrap(true);
        text_lbl.set_selectable(true);
        text_lbl.set_max_width_chars(50);

        msg_box.append(&role_lbl);
        msg_box.append(&text_lbl);
        self.chat_box.append(&msg_box);
        self.scroll_to_bottom();
    }

    fn add_streaming_placeholder(&self) -> (Label, Rc<Cell<bool>>) {
        let msg_box = GtkBox::new(Orientation::Vertical, 2);
        msg_box.set_margin_top(4);
        msg_box.set_margin_bottom(4);

        let role_lbl = Label::new(Some("AI"));
        role_lbl.set_halign(gtk4::Align::Start);
        role_lbl.add_css_class("heading");

        let text_lbl = Label::new(Some("thinking"));
        text_lbl.set_halign(gtk4::Align::Start);
        text_lbl.set_wrap(true);
        text_lbl.set_selectable(true);
        text_lbl.set_max_width_chars(50);
        text_lbl.add_css_class("dim-label");

        msg_box.append(&role_lbl);
        msg_box.append(&text_lbl);
        self.chat_box.append(&msg_box);
        self.scroll_to_bottom();

        // Animated ellipsis: "thinking" → "thinking." → "thinking.." → "thinking..."
        let anim_stop = Rc::new(Cell::new(false));
        let anim_stop_timer = anim_stop.clone();
        let anim_lbl = text_lbl.clone();
        let dot_count = Rc::new(Cell::new(0u8));
        glib::timeout_add_local(Duration::from_millis(400), move || {
            if anim_stop_timer.get() {
                return glib::ControlFlow::Break;
            }
            let n = dot_count.get();
            anim_lbl.set_text(&format!("thinking{}", ".".repeat(n as usize)));
            dot_count.set((n + 1) % 4);
            glib::ControlFlow::Continue
        });

        (text_lbl, anim_stop)
    }

    fn add_apply_buttons_for(&self, text: &str, label: &Label) {
        let blocks = extract_code_blocks(text);
        if blocks.is_empty() { return; }

        let Some(parent)  = label.parent() else { return };
        let Some(msg_box) = parent.downcast_ref::<GtkBox>() else { return };

        for (i, block) in blocks.iter().enumerate() {
            let is_xml  = matches!(block.lang.as_str(), "xml" | "ui");
            let is_rust = matches!(block.lang.as_str(), "rust" | "rs");
            let is_diff = matches!(block.lang.as_str(), "diff" | "patch");

            let code = block.code.clone();

            if is_diff {
                let diff_file = extract_diff_filename(&code);
                let btn_text = match diff_file {
                    Some(ref name) => format!("Review Diff — {}", name),
                    None => "Review Diff".to_string(),
                };
                let btn = Button::with_label(&btn_text);
                btn.add_css_class("success");
                btn.set_halign(gtk4::Align::Start);
                btn.set_margin_top(4);
                let cb = self.on_apply_diff_cb.clone();
                btn.connect_clicked(move |b| {
                    if let Some(f) = cb.borrow().as_ref() { f(&code); }
                    b.set_label("Diff opened ✓");
                    b.set_sensitive(false);
                });
                msg_box.append(&btn);
                continue;
            }

            let file_hint = extract_code_filename(&code, &block.lang);

            let btn_label = if is_xml {
                match file_hint {
                    Some(ref name) => format!("Apply to {}", name),
                    None => "Apply to .ui file".to_string(),
                }
            } else if is_rust {
                match file_hint {
                    Some(ref name) => format!("Apply to {}", name),
                    None => "Apply to .rs file".to_string(),
                }
            } else if block.lang.is_empty() {
                format!("Apply block {}", i + 1)
            } else {
                format!("Apply {} block", block.lang)
            };

            let btn = Button::with_label(&btn_label);
            btn.add_css_class("suggested-action");
            btn.set_halign(gtk4::Align::Start);
            btn.set_margin_top(4);

            if is_rust {
                let cb = self.on_apply_rs_cb.clone();
                let applied_label = match file_hint {
                    Some(ref name) => format!("Applied to {} ✓", name),
                    None => "Applied to .rs ✓".to_string(),
                };
                btn.connect_clicked(move |b| {
                    if let Some(f) = cb.borrow().as_ref() { f(&code); }
                    b.set_label(&applied_label);
                    b.set_sensitive(false);
                });
            } else {
                let cb = self.on_apply_xml_cb.clone();
                let applied_label = if is_xml {
                    match file_hint {
                        Some(ref name) => format!("Applied to {} ✓", name),
                        None => "Applied to .ui ✓".to_string(),
                    }
                } else {
                    "Applied ✓".to_string()
                };
                btn.connect_clicked(move |b| {
                    if let Some(f) = cb.borrow().as_ref() { f(&code); }
                    b.set_label(&applied_label);
                    b.set_sensitive(false);
                });
            }
            msg_box.append(&btn);
        }
    }

    fn scroll_to_bottom(&self) {
        let adj = self.scroll.vadjustment();
        glib::idle_add_local_once(move || {
            adj.set_value(adj.upper());
        });
    }
}
