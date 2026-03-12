//! claude_code.rs — Native Claude Code chat panel.
//!
//! Shells out to the `claude` CLI in print mode (`-p`) with streaming JSON
//! output.  The panel provides a scrollable chat history, a text input, and
//! an "Apply" button on code blocks to insert them into the active editor
//! buffer.  No API key required — uses the user's existing Claude Code auth.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Label, Orientation, ScrolledWindow, Separator,
    Spinner, TextView, WrapMode,
};
use gtk4::glib;
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub const SIDEBAR_WIDTH: i32 = 450;

/// System prompt that teaches Claude about Rui's .ui XML format and Rust codegen.
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

/// A single message in the chat history.
#[derive(Clone)]
struct ChatMessage {
    role:    String, // "user" or "assistant"
    content: String,
}

/// The Claude Code chat panel.
#[derive(Clone)]
pub struct ClaudeCodePanel {
    pub widget: GtkBox,
    messages:   Rc<RefCell<Vec<ChatMessage>>>,
    chat_box:   GtkBox,
    input:      TextView,
    send_btn:   Button,
    scroll:     ScrolledWindow,
    /// Callback to get the current .ui buffer contents.
    get_buffer_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>>,
    /// Callback to get the current file path.
    get_path_cb:   Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>>,
    /// Callback to get the companion .rs file contents and path.
    /// Returns (path_string, file_contents) if a companion exists.
    get_companion_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<(String, String)>>>>>,
    /// Callback to apply an XML code block to the active .ui buffer.
    on_apply_xml_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    /// Callback to apply a Rust code block — opens/creates the companion .rs file.
    on_apply_rs_cb:  Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    /// Callback to open the diff tool with a diff block.
    on_apply_diff_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    /// Whether a request is currently in flight.
    busy:          Rc<RefCell<bool>>,
    spinner:       Spinner,
    /// Handle to the currently-running Claude process (so we can kill it on close).
    child_proc:    Arc<Mutex<Option<u32>>>,  // stores PID
}

impl ClaudeCodePanel {
    pub fn new() -> Self {
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(SIDEBAR_WIDTH);
        widget.set_hexpand(false);
        widget.set_vexpand(true);
        widget.set_visible(false);

        // ── Header ─────────────────────────────────────────────────
        let header_box = GtkBox::new(Orientation::Horizontal, 6);
        header_box.set_margin_start(8);
        header_box.set_margin_end(8);
        header_box.set_margin_top(6);
        header_box.set_margin_bottom(4);

        let icon_lbl = Label::new(Some("\u{F121}")); // nf-fa-code
        icon_lbl.add_css_class("nf");
        let title = Label::new(Some("Claude Code"));
        title.add_css_class("heading");
        title.set_hexpand(true);
        title.set_halign(gtk4::Align::Start);

        let clear_btn = Button::with_label("Clear");
        clear_btn.add_css_class("flat");
        clear_btn.set_tooltip_text(Some("Clear chat history"));

        header_box.append(&icon_lbl);
        header_box.append(&title);
        header_box.append(&clear_btn);

        // ── Chat history (scrollable) ──────────────────────────────
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

        // ── Input area ─────────────────────────────────────────────
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
        let include_ui_lbl = Label::new(Some("Includes .ui + companion .rs context"));
        include_ui_lbl.add_css_class("dim-label");
        include_ui_lbl.set_halign(gtk4::Align::Start);
        include_ui_lbl.set_hexpand(true);

        let send_btn = Button::with_label("Send");
        send_btn.add_css_class("suggested-action");
        send_btn.set_tooltip_text(Some("Send to Claude Code (Ctrl+Enter)"));

        let spinner = Spinner::new();
        spinner.set_visible(false);

        btn_row.append(&include_ui_lbl);
        btn_row.append(&spinner);
        btn_row.append(&send_btn);

        input_box.append(&input_scroll);
        input_box.append(&btn_row);

        // ── Assemble ───────────────────────────────────────────────
        let sep = Separator::new(Orientation::Horizontal);
        widget.append(&header_box);
        widget.append(&sep);
        widget.append(&scroll);
        widget.append(&Separator::new(Orientation::Horizontal));
        widget.append(&input_box);

        let messages: Rc<RefCell<Vec<ChatMessage>>> = Rc::new(RefCell::new(Vec::new()));
        let get_buffer_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>> =
            Rc::new(RefCell::new(None));
        let get_path_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<String>>>>> =
            Rc::new(RefCell::new(None));
        let get_companion_cb: Rc<RefCell<Option<Box<dyn Fn() -> Option<(String, String)>>>>> =
            Rc::new(RefCell::new(None));
        let on_apply_xml_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> =
            Rc::new(RefCell::new(None));
        let on_apply_rs_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> =
            Rc::new(RefCell::new(None));
        let on_apply_diff_cb: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> =
            Rc::new(RefCell::new(None));
        let busy = Rc::new(RefCell::new(false));
        let child_proc = Arc::new(Mutex::new(None));

        let panel = ClaudeCodePanel {
            widget,
            messages,
            chat_box,
            input,
            send_btn: send_btn.clone(),
            scroll,
            get_buffer_cb,
            get_path_cb,
            get_companion_cb,
            on_apply_xml_cb,
            on_apply_rs_cb,
            on_apply_diff_cb,
            busy,
            spinner,
            child_proc,
        };

        // Wire Send button.
        {
            let p = panel.clone();
            send_btn.connect_clicked(move |_| p.do_send());
        }

        // Wire Ctrl+Enter in the input TextView.
        {
            let p = panel.clone();
            let key_ctl = gtk4::EventControllerKey::new();
            key_ctl.connect_key_pressed(move |_, key, _, mods| {
                if key == gtk4::gdk::Key::Return
                    && mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                {
                    p.do_send();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            panel.input.add_controller(key_ctl);
        }

        // Wire Clear button.
        {
            let p = panel.clone();
            clear_btn.connect_clicked(move |_| p.clear_chat());
        }

        panel
    }

    /// Register a callback to get the current .ui buffer text.
    pub fn on_get_buffer<F: Fn() -> Option<String> + 'static>(&self, cb: F) {
        *self.get_buffer_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback to get the current file path.
    pub fn on_get_path<F: Fn() -> Option<String> + 'static>(&self, cb: F) {
        *self.get_path_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback to get the companion .rs file (path, contents).
    pub fn on_get_companion<F: Fn() -> Option<(String, String)> + 'static>(&self, cb: F) {
        *self.get_companion_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback to apply an XML code block to the .ui buffer.
    pub fn on_apply_xml<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_xml_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback to apply a Rust code block to the companion .rs file.
    pub fn on_apply_rs<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_rs_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a callback to open the diff tool with a diff block.
    pub fn on_apply_diff<F: Fn(&str) + 'static>(&self, cb: F) {
        *self.on_apply_diff_cb.borrow_mut() = Some(Box::new(cb));
    }

    /// Populate the input text box (used by the robot button in the output panel).
    pub fn set_input(&self, text: &str) {
        self.input.buffer().set_text(text);
        self.input.grab_focus();
    }

    pub fn toggle(&self) -> bool {
        let visible = !self.widget.is_visible();
        self.widget.set_visible(visible);
        if visible {
            self.input.grab_focus();
        }
        visible
    }

    pub fn is_visible(&self) -> bool {
        self.widget.is_visible()
    }

    /// Kill any running Claude process. Call this on app shutdown.
    pub fn kill_process(&self) {
        if let Some(pid) = self.child_proc.lock().unwrap().take() {
            log::info!("Killing Claude process PID {}", pid);
            let _ = Command::new("kill")
                .arg(pid.to_string())
                .spawn();
        }
    }

    fn clear_chat(&self) {
        self.messages.borrow_mut().clear();
        while let Some(child) = self.chat_box.first_child() {
            self.chat_box.remove(&child);
        }
    }

    fn do_send(&self) {
        if *self.busy.borrow() {
            return;
        }

        let buf = self.input.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        // Clear input.
        buf.set_text("");

        // Get context.
        let ui_content = self.get_buffer_cb.borrow().as_ref().and_then(|cb| cb());
        let file_path  = self.get_path_cb.borrow().as_ref().and_then(|cb| cb());
        let companion  = self.get_companion_cb.borrow().as_ref().and_then(|cb| cb());

        // Build the full prompt with context.
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

        // Add user message to chat.
        self.add_message("user", &text);

        // Add a placeholder for the assistant response.
        let response_label = self.add_streaming_placeholder();

        // Mark busy.
        *self.busy.borrow_mut() = true;
        self.send_btn.set_visible(false);
        self.spinner.set_visible(true);
        self.spinner.start();

        // Spawn claude process on a background thread using async_channel.
        let (tx, rx) = async_channel::unbounded::<StreamEvent>();
        let proc_handle = self.child_proc.clone();

        std::thread::spawn(move || {
            let result = run_claude(&prompt, &tx, &proc_handle);
            // Clear the PID once the process is done.
            *proc_handle.lock().unwrap() = None;
            if let Err(e) = result {
                let _ = tx.send_blocking(StreamEvent::Error(e));
            }
            let _ = tx.send_blocking(StreamEvent::Done);
        });

        // Poll the channel from the GTK main loop.
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
                        let current = accumulated.borrow().clone();
                        let err_text = if current.is_empty() {
                            format!("[Error: {}]", msg)
                        } else {
                            format!("{}\n\n[Error: {}]", current, msg)
                        };
                        response_label.set_text(&err_text);
                        response_label.add_css_class("error");
                    }
                    StreamEvent::Done => {
                        *panel.busy.borrow_mut() = false;
                        panel.spinner.stop();
                        panel.spinner.set_visible(false);
                        panel.send_btn.set_visible(true);

                        let final_text = accumulated.borrow().clone();
                        if !final_text.is_empty() {
                            panel.messages.borrow_mut().push(ChatMessage {
                                role: "assistant".into(),
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
            role: role.into(),
            content: content.into(),
        });

        let msg_box = GtkBox::new(Orientation::Vertical, 2);
        msg_box.set_margin_top(4);
        msg_box.set_margin_bottom(4);

        let role_lbl = Label::new(Some(if role == "user" { "You" } else { "Claude" }));
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

    fn add_streaming_placeholder(&self) -> Label {
        let msg_box = GtkBox::new(Orientation::Vertical, 2);
        msg_box.set_margin_top(4);
        msg_box.set_margin_bottom(4);

        let role_lbl = Label::new(Some("Claude"));
        role_lbl.set_halign(gtk4::Align::Start);
        role_lbl.add_css_class("heading");

        let text_lbl = Label::new(Some("thinking…"));
        text_lbl.set_halign(gtk4::Align::Start);
        text_lbl.set_wrap(true);
        text_lbl.set_selectable(true);
        text_lbl.set_max_width_chars(50);
        text_lbl.add_css_class("dim-label");

        msg_box.append(&role_lbl);
        msg_box.append(&text_lbl);
        self.chat_box.append(&msg_box);
        self.scroll_to_bottom();
        text_lbl
    }

    /// After streaming completes, scan for code blocks and add "Apply" buttons.
    /// XML blocks → apply to .ui buffer, Rust blocks → apply to companion .rs.
    fn add_apply_buttons_for(&self, text: &str, label: &Label) {
        let blocks = extract_code_blocks(text);
        if blocks.is_empty() {
            return;
        }

        // The label's parent is the msg_box.
        let Some(parent) = label.parent() else { return };
        let Some(msg_box) = parent.downcast_ref::<GtkBox>() else { return };

        for (i, block) in blocks.iter().enumerate() {
            let is_xml  = matches!(block.lang.as_str(), "xml" | "ui");
            let is_rust = matches!(block.lang.as_str(), "rust" | "rs");
            let is_diff = matches!(block.lang.as_str(), "diff" | "patch");

            let code = block.code.clone();

            if is_diff {
                // Green "Review Diff" button → opens diff tool
                let btn = Button::with_label("Review Diff");
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

            let btn_label = if is_xml {
                "Apply to .ui file".to_string()
            } else if is_rust {
                "Apply to .rs file".to_string()
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
                let apply_cb = self.on_apply_rs_cb.clone();
                btn.connect_clicked(move |b| {
                    if let Some(cb) = apply_cb.borrow().as_ref() {
                        cb(&code);
                    }
                    b.set_label("Applied to .rs ✓");
                    b.set_sensitive(false);
                });
            } else {
                let apply_cb = self.on_apply_xml_cb.clone();
                btn.connect_clicked(move |b| {
                    if let Some(cb) = apply_cb.borrow().as_ref() {
                        cb(&code);
                    }
                    b.set_label(if is_xml { "Applied to .ui ✓" } else { "Applied ✓" });
                    b.set_sensitive(false);
                });
            }

            msg_box.append(&btn);
        }
    }

    fn scroll_to_bottom(&self) {
        let adj = self.scroll.vadjustment();
        // Schedule scroll after layout settles.
        glib::idle_add_local_once(move || {
            adj.set_value(adj.upper());
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Claude CLI interface
// ─────────────────────────────────────────────────────────────────────────────

enum StreamEvent {
    Text(String),
    Error(String),
    Done,
}

/// Spawn `claude -p` and stream the result back via `tx`.
fn run_claude(
    prompt: &str,
    tx: &async_channel::Sender<StreamEvent>,
    proc_handle: &Arc<Mutex<Option<u32>>>,
) -> Result<(), String> {
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
       .arg(prompt)
       .arg("--verbose")
       .arg("--output-format")
       .arg("stream-json")
       .arg("--system-prompt")
       .arg(SYSTEM_PROMPT)
       .stdin(Stdio::null())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Claude Code CLI not found. Install it with: npm install -g @anthropic-ai/claude-code".into()
        } else {
            format!("Failed to spawn claude: {}", e)
        }
    })?;

    // Store the PID so we can kill it on app close.
    *proc_handle.lock().unwrap() = Some(child.id());

    let stdout = child.stdout.take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let reader = BufReader::new(stdout);
    let mut got_streamed_text = false;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                let _ = tx.send_blocking(StreamEvent::Error(format!("Read error: {}", e)));
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        // Parse each JSON line.
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(val) => {
                let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                log::debug!("claude stream event: type={}", msg_type);

                match msg_type {
                    "assistant" => {
                        // Streaming text chunk: message.content[].text
                        if let Some(content) = val.pointer("/message/content") {
                            if let Some(arr) = content.as_array() {
                                for item in arr {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        got_streamed_text = true;
                                        let _ = tx.send_blocking(StreamEvent::Text(text.to_string()));
                                    }
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(text) = val.pointer("/delta/text").and_then(|t| t.as_str()) {
                            got_streamed_text = true;
                            let _ = tx.send_blocking(StreamEvent::Text(text.to_string()));
                        }
                    }
                    "result" => {
                        // Final result — only use if we didn't get streamed text.
                        if let Some(result) = val.get("result").and_then(|r| r.as_str()) {
                            if !got_streamed_text {
                                let _ = tx.send_blocking(StreamEvent::Text(result.to_string()));
                            }
                        }
                    }
                    _ => {
                        log::debug!("claude: ignoring event type '{}'", msg_type);
                    }
                }
            }
            Err(e) => {
                log::warn!("claude: failed to parse JSON line: {} — {}", e, &line[..line.len().min(200)]);
            }
        }
    }

    // Wait for the process to finish.
    let status = child.wait().map_err(|e| format!("Wait error: {}", e))?;
    if !status.success() {
        // Try to read stderr.
        if let Some(mut stderr) = child.stderr.take() {
            let mut err = String::new();
            use std::io::Read;
            let _ = stderr.read_to_string(&mut err);
            if !err.trim().is_empty() {
                let _ = tx.send_blocking(StreamEvent::Error(err.trim().to_string()));
            }
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Code block extraction
// ─────────────────────────────────────────────────────────────────────────────

struct CodeBlock {
    lang: String,
    code: String,
}

/// Extract fenced code blocks from markdown text.
fn extract_code_blocks(text: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") && trimmed.len() > 3 {
            let lang = trimmed[3..].trim().to_string();
            let mut code = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" {
                    break;
                }
                if !code.is_empty() {
                    code.push('\n');
                }
                code.push_str(inner);
            }
            if !code.is_empty() {
                blocks.push(CodeBlock { lang, code });
            }
        }
    }

    blocks
}
