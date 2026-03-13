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
    Popover, ScrolledWindow, Separator, Spinner, StringList, Switch, TextView, WrapMode,
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
- Keep responses concise and code-focused.
- Before returning any code, silently review it for syntax errors, missing imports, wrong method names, and type mismatches. Fix all issues before responding — do not return broken code."#;


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

// ── Message rendering ────────────────────────────────────────────────────────

enum MsgSegment {
    Text(String),
    Code { lang: String, code: String },
}

fn parse_message_segments(text: &str) -> Vec<MsgSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if !current.trim().is_empty() {
                segments.push(MsgSegment::Text(current.trim_end().to_string()));
                current = String::new();
            }
            let lang = trimmed[3..].trim().to_string();
            let mut code = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" { break; }
                if !code.is_empty() { code.push('\n'); }
                code.push_str(inner);
            }
            if !code.is_empty() {
                segments.push(MsgSegment::Code { lang, code });
            }
        } else {
            if !current.is_empty() { current.push('\n'); }
            current.push_str(line);
        }
    }
    if !current.trim().is_empty() {
        segments.push(MsgSegment::Text(current.trim_end().to_string()));
    }
    segments
}

fn render_response(msg_box: &gtk4::Box, streaming_view: &TextView, text: &str) {
    let segments = parse_message_segments(text);

    if segments.len() == 1 {
        if let MsgSegment::Text(ref t) = segments[0] {
            apply_markup_to_view(streaming_view, t);
            return;
        }
    }

    msg_box.remove(streaming_view);

    for segment in &segments {
        match segment {
            MsgSegment::Text(t) => {
                if t.trim().is_empty() { continue; }
                let tv = TextView::new();
                tv.set_editable(false);
                tv.set_cursor_visible(false);
                tv.set_wrap_mode(WrapMode::WordChar);
                tv.set_hexpand(true);
                tv.set_left_margin(2);
                tv.set_right_margin(2);
                tv.set_top_margin(2);
                tv.add_css_class("chat-body");
                apply_markup_to_view(&tv, t);
                msg_box.append(&tv);
            }
            MsgSegment::Code { lang, code } => {
                let code_box = gtk4::Box::new(Orientation::Vertical, 0);
                code_box.add_css_class("chat-code-block");
                code_box.set_margin_top(4);
                code_box.set_margin_bottom(4);
                code_box.set_hexpand(true);

                if !lang.is_empty() {
                    let lang_lbl = Label::new(Some(lang));
                    lang_lbl.set_halign(gtk4::Align::Start);
                    lang_lbl.add_css_class("chat-code-lang");
                    code_box.append(&lang_lbl);
                }

                let tv = TextView::new();
                tv.set_editable(false);
                tv.set_cursor_visible(false);
                tv.set_monospace(true);
                tv.set_wrap_mode(WrapMode::None);
                tv.set_hexpand(true);
                tv.set_left_margin(8);
                tv.set_right_margin(8);
                tv.set_top_margin(6);
                tv.set_bottom_margin(6);
                tv.add_css_class("chat-code-view");
                tv.buffer().set_text(code);

                let sw = ScrolledWindow::builder()
                    .hexpand(true)
                    .vscrollbar_policy(gtk4::PolicyType::Never)
                    .hscrollbar_policy(gtk4::PolicyType::Automatic)
                    .child(&tv)
                    .build();
                code_box.append(&sw);
                msg_box.append(&code_box);
            }
        }
    }
}

fn strip_inline_markdown(text: &str) -> (String, Vec<(i32, i32, &'static str)>) {
    let mut out = String::new();
    let mut spans: Vec<(i32, i32, &'static str)> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut out_len: i32 = 0;
    while i < n {
        if i + 1 < n && chars[i] == '*' && chars[i + 1] == '*' {
            let inner = i + 2;
            let mut close = None;
            let mut j = inner;
            while j + 1 < n {
                if chars[j] == '*' && chars[j + 1] == '*' { close = Some(j); break; }
                j += 1;
            }
            if let Some(c) = close {
                let start = out_len;
                for k in inner..c { out.push(chars[k]); out_len += 1; }
                if out_len > start { spans.push((start, out_len, "b")); }
                i = c + 2;
                continue;
            }
        }
        if chars[i] == '*' && (i + 1 >= n || chars[i + 1] != '*') {
            let inner = i + 1;
            let mut close = None;
            let mut j = inner;
            while j < n {
                if chars[j] == '*' && (j + 1 >= n || chars[j + 1] != '*') { close = Some(j); break; }
                j += 1;
            }
            if let Some(c) = close {
                let start = out_len;
                for k in inner..c { out.push(chars[k]); out_len += 1; }
                if out_len > start { spans.push((start, out_len, "i")); }
                i = c + 1;
                continue;
            }
        }
        out.push(chars[i]);
        out_len += 1;
        i += 1;
    }
    (out, spans)
}

fn apply_markup_to_view(view: &TextView, text: &str) {
    let buf = view.buffer();
    let table = buf.tag_table();
    if table.lookup("b").is_none() {
        if let Some(t) = buf.create_tag(Some("b"), &[]) {
            t.set_weight(700);
        }
    }
    if table.lookup("i").is_none() {
        if let Some(t) = buf.create_tag(Some("i"), &[]) {
            t.set_style(gtk4::pango::Style::Italic);
        }
    }
    let (plain, spans) = strip_inline_markdown(text);
    buf.set_text(&plain);
    for (s, e, name) in &spans {
        if let Some(tag) = table.lookup(name) {
            buf.apply_tag(&tag, &buf.iter_at_offset(*s), &buf.iter_at_offset(*e));
        }
    }
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
    stop_btn:   Button,
    ctx_lbl:    Label,
    warn_lbl:   Label,
    file_switch: Switch,
    /// True after files have been sent once; reset on clear or switch toggle-off.
    files_sent:  Rc<Cell<bool>>,
    /// Shared session history panel (optional — wired after construction).
    history:     Rc<RefCell<Option<crate::chat_history::ChatHistoryPanel>>>,
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

        let stop_btn = Button::with_label("Stop");
        stop_btn.add_css_class("destructive-action");
        stop_btn.set_tooltip_text(Some("Stop generation"));
        stop_btn.set_visible(false);

        let send_btn = Button::with_label("Send");
        send_btn.add_css_class("suggested-action");
        send_btn.set_tooltip_text(Some("Send (Ctrl+Enter)"));

        let spinner = Spinner::new();
        spinner.set_visible(false);

        btn_row.append(&ctx_lbl);
        btn_row.append(&spinner);
        btn_row.append(&stop_btn);
        btn_row.append(&send_btn);

        let warn_lbl = Label::new(Some(
            "Long conversation — context may be truncated. Consider clearing.",
        ));
        warn_lbl.add_css_class("chat-warn");
        warn_lbl.set_wrap(true);
        warn_lbl.set_visible(false);

        // File context toggle row
        let files_row = GtkBox::new(Orientation::Horizontal, 6);
        files_row.set_margin_start(2);
        let file_switch = Switch::new();
        file_switch.set_active(true);
        file_switch.set_tooltip_text(Some("Include .ui + .rs file contents as context"));
        file_switch.set_valign(gtk4::Align::Center);
        let files_lbl = Label::new(Some("Send file context"));
        files_lbl.add_css_class("dim-label");
        files_lbl.set_halign(gtk4::Align::Start);
        files_row.append(&file_switch);
        files_row.append(&files_lbl);

        input_box.append(&warn_lbl);
        input_box.append(&files_row);
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
            stop_btn:         stop_btn.clone(),
            ctx_lbl:          ctx_lbl.clone(),
            warn_lbl:         warn_lbl.clone(),
            file_switch:      file_switch.clone(),
            files_sent:       Rc::new(Cell::new(false)),
            history:          Rc::new(RefCell::new(None)),
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

        // Wire Stop button
        {
            let p = panel.clone();
            stop_btn.connect_clicked(move |_| {
                *p.busy.borrow_mut() = false;
                p.spinner.stop();
                p.spinner.set_visible(false);
                p.send_btn.set_visible(true);
                p.stop_btn.set_visible(false);
            });
        }

        // Reset files_sent when switch is turned off.
        {
            let p = panel.clone();
            file_switch.connect_active_notify(move |sw| {
                if !sw.is_active() { p.files_sent.set(false); }
            });
        }

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

    pub fn set_history(&self, h: crate::chat_history::ChatHistoryPanel) {
        *self.history.borrow_mut() = Some(h);
    }

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
        self.files_sent.set(false);
        self.ctx_lbl.set_text("Includes .ui + companion .rs context");
        self.warn_lbl.set_visible(false);
    }

    fn update_ctx_lbl(&self) {
        let msgs = self.messages.borrow();
        let count = msgs.len();
        let chars: usize = msgs.iter().map(|m| m.content.len()).sum();
        if count == 0 {
            self.ctx_lbl.set_text("Includes .ui + companion .rs context");
        } else {
            let k = chars / 1000;
            if k > 0 {
                self.ctx_lbl.set_text(&format!("{} msgs · ~{}k chars", count, k));
            } else {
                self.ctx_lbl.set_text(&format!("{} msgs · {} chars", count, chars));
            }
        }
        self.warn_lbl.set_visible(count >= 10);
    }

    fn do_send(&self) {
        if *self.busy.borrow() { return; }

        let buf  = self.input.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let text = text.trim().to_string();
        if text.is_empty() { return; }
        buf.set_text("");

        // Gather context — only on first message per conversation (files_sent flag).
        let send_files = self.file_switch.is_active() && !self.files_sent.get();
        let ui_content = send_files.then(|| self.get_buffer_cb.borrow().as_ref().and_then(|cb| cb())).flatten();
        let file_path  = send_files.then(|| self.get_path_cb.borrow().as_ref().and_then(|cb| cb())).flatten();
        let companion  = send_files.then(|| self.get_companion_cb.borrow().as_ref().and_then(|cb| cb())).flatten();

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

        // Mark files as sent so subsequent messages don't repeat the full file dump.
        if send_files { self.files_sent.set(true); }

        self.add_message("user", &text);
        let (response_view, anim_stop) = self.add_streaming_placeholder();

        *self.busy.borrow_mut() = true;
        self.send_btn.set_visible(false);
        self.stop_btn.set_visible(true);
        self.spinner.set_visible(true);
        self.spinner.start();
        self.update_ctx_lbl();

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
        let first_chunk = Rc::new(Cell::new(true));
        glib::idle_add_local(move || {
            while let Ok(event) = rx.try_recv() {
                match event {
                    StreamEvent::Text(chunk) => {
                        let buf = response_view.buffer();
                        if first_chunk.get() {
                            first_chunk.set(false);
                            anim_stop.set(true);
                            response_view.remove_css_class("dim-label");
                            buf.set_text(&chunk);
                        } else {
                            let mut end = buf.end_iter();
                            buf.insert(&mut end, &chunk);
                        }
                        panel.scroll_to_bottom();
                    }
                    StreamEvent::Error(msg) => {
                        let buf = response_view.buffer();
                        let cur = buf.text(&buf.start_iter(), &buf.end_iter(), false);
                        let err = if cur.is_empty() || first_chunk.get() {
                            format!("[Error: {}]", msg)
                        } else {
                            format!("{}\n\n[Error: {}]", cur, msg)
                        };
                        buf.set_text(&err);
                        response_view.add_css_class("error");
                    }
                    StreamEvent::Done => {
                        anim_stop.set(true);
                        *panel.busy.borrow_mut() = false;
                        panel.spinner.stop();
                        panel.spinner.set_visible(false);
                        panel.send_btn.set_visible(true);
                        panel.stop_btn.set_visible(false);

                        let buf = response_view.buffer();
                        let final_text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
                        if !final_text.is_empty() && !first_chunk.get() {
                            panel.messages.borrow_mut().push(ChatMessage {
                                role:    "assistant".into(),
                                content: final_text.clone(),
                            });
                            if let Some(mb) = response_view.parent() {
                                if let Some(msg_box) = mb.downcast_ref::<GtkBox>() {
                                    render_response(msg_box, &response_view, &final_text);
                                    panel.add_apply_buttons_for(&final_text, msg_box);
                                }
                            }
                            panel.update_ctx_lbl();
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
        msg_box.set_margin_top(6);
        msg_box.set_margin_bottom(6);
        msg_box.set_margin_start(4);
        msg_box.set_margin_end(4);
        if role == "user" {
            msg_box.add_css_class("chat-user-msg");
        }

        // Header row: role label + copy button
        let header_row = GtkBox::new(Orientation::Horizontal, 4);
        let role_lbl = Label::new(Some(if role == "user" { "You" } else { "AI" }));
        role_lbl.set_halign(gtk4::Align::Start);
        role_lbl.set_hexpand(true);
        role_lbl.add_css_class("heading");
        header_row.append(&role_lbl);

        let copy_btn = gtk4::Button::builder()
            .icon_name("edit-copy-symbolic")
            .has_frame(false)
            .tooltip_text("Copy to clipboard")
            .build();
        copy_btn.add_css_class("flat");
        let copy_text = content.to_string();
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gtk4::gdk::Display::default() {
                display.clipboard().set_text(&copy_text);
            }
        });
        header_row.append(&copy_btn);

        // Body as non-editable TextView — appending doesn't trigger relayout
        let text_view = TextView::new();
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_wrap_mode(WrapMode::WordChar);
        text_view.set_hexpand(true);
        text_view.set_left_margin(2);
        text_view.set_right_margin(2);
        text_view.set_top_margin(2);
        text_view.add_css_class("chat-body");
        apply_markup_to_view(&text_view, content);

        msg_box.append(&header_row);
        msg_box.append(&text_view);
        self.chat_box.append(&msg_box);
        self.update_ctx_lbl();
        self.scroll_to_bottom();
    }

    fn add_streaming_placeholder(&self) -> (TextView, Rc<Cell<bool>>) {
        let msg_box = GtkBox::new(Orientation::Vertical, 2);
        msg_box.set_margin_top(6);
        msg_box.set_margin_bottom(6);
        msg_box.set_margin_start(4);
        msg_box.set_margin_end(4);

        let role_lbl = Label::new(Some("AI"));
        role_lbl.set_halign(gtk4::Align::Start);
        role_lbl.add_css_class("heading");

        let text_view = TextView::new();
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_wrap_mode(WrapMode::WordChar);
        text_view.set_hexpand(true);
        text_view.set_left_margin(2);
        text_view.set_right_margin(2);
        text_view.set_top_margin(2);
        text_view.add_css_class("chat-body");
        text_view.add_css_class("dim-label");
        text_view.buffer().set_text("thinking");

        msg_box.append(&role_lbl);
        msg_box.append(&text_view);
        self.chat_box.append(&msg_box);
        self.scroll_to_bottom();

        // Animated ellipsis via buffer updates (no relayout)
        let anim_stop = Rc::new(Cell::new(false));
        let anim_stop_timer = anim_stop.clone();
        let anim_buf = text_view.buffer();
        let dot_count = Rc::new(Cell::new(0u8));
        glib::timeout_add_local(Duration::from_millis(400), move || {
            if anim_stop_timer.get() {
                return glib::ControlFlow::Break;
            }
            let n = dot_count.get();
            anim_buf.set_text(&format!("thinking{}", ".".repeat(n as usize)));
            dot_count.set((n + 1) % 4);
            glib::ControlFlow::Continue
        });

        (text_view, anim_stop)
    }

    fn add_apply_buttons_for(&self, text: &str, msg_box: &GtkBox) {
        // Add copy button to header row (always, even if no code blocks)
        if let Some(header) = msg_box.first_child() {
            if let Some(header_row) = header.downcast_ref::<GtkBox>() {
                let copy_btn = gtk4::Button::builder()
                    .icon_name("edit-copy-symbolic")
                    .has_frame(false)
                    .tooltip_text("Copy response")
                    .build();
                copy_btn.add_css_class("flat");
                let copy_text = text.to_string();
                copy_btn.connect_clicked(move |_| {
                    if let Some(display) = gtk4::gdk::Display::default() {
                        display.clipboard().set_text(&copy_text);
                    }
                });
                header_row.append(&copy_btn);
            }
        }

        let blocks = extract_code_blocks(text);
        if blocks.is_empty() { return; }

        // Record every block in the session history panel.
        if let Some(ref h) = *self.history.borrow() {
            for block in &blocks {
                let display_name = if matches!(block.lang.as_str(), "diff" | "patch") {
                    extract_diff_filename(&block.code)
                } else {
                    extract_code_filename(&block.code, &block.lang)
                        .or_else(|| {
                            self.get_path_cb.borrow().as_ref().and_then(|cb| cb())
                                .and_then(|p| {
                                    if matches!(block.lang.as_str(), "rust" | "rs") {
                                        std::path::Path::new(&p).with_extension("rs")
                                            .file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                                    } else {
                                        std::path::Path::new(&p).file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                                    }
                                })
                        })
                };
                h.add_entry("AI Chat", &block.lang, display_name, block.code.clone());
            }
        }

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

            let file_hint = extract_code_filename(&code, &block.lang).or_else(|| {
                self.get_path_cb.borrow().as_ref().and_then(|cb| cb()).and_then(|p| {
                    if is_rust {
                        std::path::Path::new(&p).with_extension("rs")
                            .file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                    } else {
                        std::path::Path::new(&p).file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                    }
                })
            });

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
