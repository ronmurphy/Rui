//! lsp_client.rs — rust-analyzer LSP client for Rui.
//!
//! Spawns `rust-analyzer` as a subprocess and communicates via JSON-RPC (LSP).
//! All GTK state is only touched on the GTK main thread.
//! Subprocess I/O runs in a dedicated tokio runtime on background threads.
//! The two worlds communicate via `async_channel`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

use lsp_types::*;
use serde_json::Value;

use crate::diagnostics;
use crate::notebook::NotebookManager;

// ── Internal message: tokio → GTK main loop ───────────────────────────────────

enum LspMsg {
    Response { id: u64, result: Option<Value> },
    Notification { method: String, params: Value },
    Shutdown,
}

// ── Public struct ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LspClient {
    /// Send raw JSON-RPC frames to rust-analyzer's stdin.
    sender:       async_channel::Sender<String>,
    next_id:      Rc<Cell<u64>>,
    /// Pending request callbacks keyed by request id.
    pending:      Rc<RefCell<HashMap<u64, Box<dyn FnOnce(Option<Value>)>>>>,
    /// True once the initialize/initialized handshake completes.
    pub initialized: Rc<Cell<bool>>,
    /// Per-file document version counter (LSP requires monotonically increasing).
    doc_versions: Rc<RefCell<HashMap<PathBuf, i32>>>,
    /// Keep the tokio runtime alive for the lifetime of the client.
    _rt: Arc<tokio::runtime::Runtime>,
}

impl LspClient {
    // ── Startup ───────────────────────────────────────────────────────────────

    /// Attempt to start rust-analyzer for `root`.
    /// Returns `None` if rust-analyzer is not installed or fails to start.
    pub fn start(
        root: &Path,
        notebook: NotebookManager,
        last_diags: Rc<RefCell<Vec<diagnostics::Diagnostic>>>,
    ) -> Option<Self> {
        // Quick check: is rust-analyzer on PATH?
        if std::process::Command::new("rust-analyzer")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            log::warn!("rust-analyzer not found — LSP features unavailable");
            return None;
        }

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("rui-lsp")
                .build()
                .ok()?,
        );

        let root_uri = Url::from_file_path(root).ok()?;
        let root_path = root.to_path_buf();

        // GTK→tokio: outgoing JSON-RPC text
        let (out_tx, out_rx) = async_channel::unbounded::<String>();
        // tokio→GTK: parsed incoming messages
        let (in_tx, in_rx) = async_channel::unbounded::<LspMsg>();

        // Spawn the I/O tasks in the tokio runtime
        {
            let in_tx2 = in_tx.clone();
            rt.spawn(async move {
                run_io(root_path, out_rx, in_tx2).await;
            });
        }

        let client = Self {
            sender:       out_tx,
            next_id:      Rc::new(Cell::new(1)),
            pending:      Rc::new(RefCell::new(HashMap::new())),
            initialized:  Rc::new(Cell::new(false)),
            doc_versions: Rc::new(RefCell::new(HashMap::new())),
            _rt:          rt,
        };

        // Start the GTK-thread receive loop
        {
            let client2     = client.clone();
            let notebook2   = notebook.clone();
            let last_diags2 = last_diags.clone();
            gtk4::glib::spawn_future_local(async move {
                while let Ok(msg) = in_rx.recv().await {
                    match msg {
                        LspMsg::Response { id, result } => {
                            if let Some(cb) = client2.pending.borrow_mut().remove(&id) {
                                cb(result);
                            }
                        }
                        LspMsg::Notification { method, params } => {
                            client2.handle_notification(&method, params, &notebook2, &last_diags2);
                        }
                        LspMsg::Shutdown => break,
                    }
                }
            });
        }

        // Send the initialize request
        client.send_initialize(root_uri);
        Some(client)
    }

    // ── Document lifecycle notifications ──────────────────────────────────────

    pub fn notify_open(&self, path: &Path, text: &str) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        let version = self.bump_version(path);
        let lang = if path.extension().map(|e| e == "rs").unwrap_or(false) { "rust" } else { "text" };
        self.send_notification("textDocument/didOpen", serde_json::json!({
            "textDocument": {
                "uri":        uri.as_str(),
                "languageId": lang,
                "version":    version,
                "text":       text,
            }
        }));
    }

    pub fn notify_change(&self, path: &Path, text: &str) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        let version = self.bump_version(path);
        self.send_notification("textDocument/didChange", serde_json::json!({
            "textDocument": { "uri": uri.as_str(), "version": version },
            "contentChanges": [{ "text": text }]
        }));
    }

    pub fn notify_close(&self, path: &Path) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        self.doc_versions.borrow_mut().remove(path);
        self.send_notification("textDocument/didClose", serde_json::json!({
            "textDocument": { "uri": uri.as_str() }
        }));
    }

    // ── Requests ──────────────────────────────────────────────────────────────

    /// Request hover info at `(line, col)` (0-based). Calls `on_result` with
    /// plain-text hover content, or `None` if unavailable.
    pub fn request_hover(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        on_result: impl FnOnce(Option<String>) + 'static,
    ) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        self.send_request("textDocument/hover", serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": col }
        }), move |result| {
            let text = result
                .and_then(|v| serde_json::from_value::<Hover>(v).ok())
                .and_then(|h| hover_to_string(&h));
            on_result(text);
        });
    }

    /// Request go-to-definition at `(line, col)` (0-based). Calls `on_result`
    /// with `(file_path, line, col)`, or `None` if not found.
    pub fn request_definition(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        on_result: impl FnOnce(Option<(PathBuf, u32, u32)>) + 'static,
    ) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        self.send_request("textDocument/definition", serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": col }
        }), move |result| {
            let loc = result.and_then(|v| {
                // RA returns Location | Location[] | LocationLink[]
                if let Ok(l) = serde_json::from_value::<Location>(v.clone()) {
                    Some(l)
                } else if let Ok(ls) = serde_json::from_value::<Vec<Location>>(v.clone()) {
                    ls.into_iter().next()
                } else if let Ok(ls) = serde_json::from_value::<Vec<LocationLink>>(v) {
                    ls.into_iter().next().map(|l| Location { uri: l.target_uri, range: l.target_range })
                } else {
                    None
                }
            }).and_then(|loc| {
                loc.uri.to_file_path().ok().map(|p| {
                    (p, loc.range.start.line, loc.range.start.character)
                })
            });
            on_result(loc);
        });
    }

    /// Request completions at `(line, col)` (0-based).
    pub fn request_completion(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        on_result: impl FnOnce(Vec<CompletionItem>) + 'static,
    ) {
        if !self.initialized.get() { return; }
        let Ok(uri) = Url::from_file_path(path) else { return };
        self.send_request("textDocument/completion", serde_json::json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": col }
        }), move |result| {
            let items = result.and_then(|v| {
                // CompletionList | CompletionItem[]
                if let Some(arr) = v.as_array() {
                    serde_json::from_value::<Vec<CompletionItem>>(Value::Array(arr.clone())).ok()
                } else if let Some(items) = v.get("items") {
                    serde_json::from_value::<Vec<CompletionItem>>(items.clone()).ok()
                } else {
                    None
                }
            }).unwrap_or_default();
            on_result(items);
        });
    }

    // ── Shutdown ──────────────────────────────────────────────────────────────

    pub fn shutdown(&self) {
        let sender = self.sender.clone();
        self.send_request("shutdown", serde_json::Value::Null, move |_| {
            let _ = sender.try_send(serde_json::json!({
                "jsonrpc": "2.0",
                "method":  "exit",
                "params":  null
            }).to_string());
        });
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn send_initialize(&self, root_uri: Url) {
        let init_rc = self.initialized.clone();
        let sender  = self.sender.clone();

        self.send_request("initialize", serde_json::json!({
            "processId": std::process::id(),
            "rootUri":   root_uri.as_str(),
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "didSave": true,
                        "dynamicRegistration": false
                    },
                    "publishDiagnostics": {
                        "relatedInformation": false
                    },
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["plaintext"]
                    },
                    "completion": {
                        "dynamicRegistration": false,
                        "completionItem": {
                            "snippetSupport": false,
                            "documentationFormat": ["plaintext"]
                        }
                    },
                    "definition": { "dynamicRegistration": false }
                }
            }
        }), move |_| {
            // Send initialized notification
            let _ = sender.try_send(serde_json::json!({
                "jsonrpc": "2.0",
                "method":  "initialized",
                "params":  {}
            }).to_string());
            init_rc.set(true);
            log::info!("LSP: rust-analyzer initialized ✓");
        });
    }

    fn handle_notification(
        &self,
        method: &str,
        params: Value,
        notebook: &NotebookManager,
        last_diags: &Rc<RefCell<Vec<diagnostics::Diagnostic>>>,
    ) {
        if method == "textDocument/publishDiagnostics" {
            let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(params) else { return };
            let Ok(path) = p.uri.to_file_path() else { return };

            let new_diags: Vec<diagnostics::Diagnostic> = p.diagnostics.iter()
                .map(|d| lsp_diag_to_ours(d, &path))
                .collect();

            // Merge: replace all diags for this file, keep others
            {
                let mut stored = last_diags.borrow_mut();
                stored.retain(|d| d.file != path);
                stored.extend(new_diags.iter().cloned());
            }

            // Apply inline marks to any open tab for this file
            let stored = last_diags.borrow();
            for tab in notebook.all_tabs() {
                if tab.path().as_deref() == Some(path.as_path()) {
                    diagnostics::apply_to_tab(&tab, &stored);
                }
            }
        }
    }

    fn send_notification(&self, method: &str, params: Value) {
        let _ = self.sender.try_send(serde_json::json!({
            "jsonrpc": "2.0",
            "method":  method,
            "params":  params,
        }).to_string());
    }

    fn send_request<F>(&self, method: &str, params: Value, on_result: F)
    where
        F: FnOnce(Option<Value>) + 'static,
    {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.pending.borrow_mut().insert(id, Box::new(on_result));
        let _ = self.sender.try_send(serde_json::json!({
            "jsonrpc": "2.0",
            "id":      id,
            "method":  method,
            "params":  params,
        }).to_string());
    }

    fn bump_version(&self, path: &Path) -> i32 {
        let mut map = self.doc_versions.borrow_mut();
        let v = map.entry(path.to_path_buf()).or_insert(0);
        *v += 1;
        *v
    }
}

// ── tokio I/O tasks ───────────────────────────────────────────────────────────

async fn run_io(
    root: PathBuf,
    out_rx: async_channel::Receiver<String>,
    in_tx:  async_channel::Sender<LspMsg>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
    use tokio::process::Command;

    let mut child = match Command::new("rust-analyzer")
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("LSP: failed to spawn rust-analyzer: {}", e);
            let _ = in_tx.send(LspMsg::Shutdown).await;
            return;
        }
    };

    let stdin  = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    // Writer task: channel → stdin
    let mut writer = BufWriter::new(stdin);
    let write_task = tokio::spawn(async move {
        while let Ok(msg) = out_rx.recv().await {
            let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
            if writer.write_all(frame.as_bytes()).await.is_err() { break; }
            if writer.flush().await.is_err() { break; }
        }
    });

    // Reader loop: stdout → channel
    let mut reader = BufReader::new(stdout);
    loop {
        // Parse Content-Length header
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 { goto_shutdown(&in_tx, &write_task).await; return; }
            let trimmed = line.trim();
            if trimmed.is_empty() { break; }
            if let Some(rest) = trimmed.strip_prefix("Content-Length: ") {
                content_length = rest.trim().parse().unwrap_or(0);
            }
        }
        if content_length == 0 { continue; }

        // Read body
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).await.is_err() {
            goto_shutdown(&in_tx, &write_task).await;
            return;
        }
        let Ok(text) = std::str::from_utf8(&body) else { continue };
        let Ok(val)  = serde_json::from_str::<Value>(text) else { continue };

        let msg = if val.get("id").map(|v| !v.is_null()).unwrap_or(false) {
            LspMsg::Response {
                id:     val["id"].as_u64().unwrap_or(0),
                result: val.get("result").cloned(),
            }
        } else if let Some(method) = val["method"].as_str() {
            LspMsg::Notification {
                method: method.to_string(),
                params: val["params"].clone(),
            }
        } else {
            continue;
        };

        if in_tx.send(msg).await.is_err() { break; }
    }

    goto_shutdown(&in_tx, &write_task).await;
}

async fn goto_shutdown(
    in_tx:      &async_channel::Sender<LspMsg>,
    write_task: &tokio::task::JoinHandle<()>,
) {
    let _ = in_tx.send(LspMsg::Shutdown).await;
    write_task.abort();
}

// ── Conversion helpers ────────────────────────────────────────────────────────

fn lsp_diag_to_ours(d: &lsp_types::Diagnostic, file: &Path) -> diagnostics::Diagnostic {
    let level = match d.severity {
        Some(DiagnosticSeverity::ERROR)   => diagnostics::DiagLevel::Error,
        Some(DiagnosticSeverity::WARNING) => diagnostics::DiagLevel::Warning,
        _                                 => diagnostics::DiagLevel::Note,
    };
    diagnostics::Diagnostic {
        file:       file.to_path_buf(),
        line_start: d.range.start.line + 1,
        col_start:  d.range.start.character + 1,
        line_end:   d.range.end.line + 1,
        col_end:    d.range.end.character + 1,
        level,
        message:    d.message.clone(),
        rendered:   String::new(),
    }
}

fn hover_to_string(h: &Hover) -> Option<String> {
    match &h.contents {
        HoverContents::Scalar(ms) => Some(marked_string_to_str(ms)),
        HoverContents::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(marked_string_to_str).collect();
            if parts.is_empty() { None } else { Some(parts.join("\n─\n")) }
        }
        HoverContents::Markup(mc) => Some(mc.value.clone()),
    }
}

fn marked_string_to_str(ms: &MarkedString) -> String {
    match ms {
        MarkedString::String(s)         => s.clone(),
        MarkedString::LanguageString(ls) => ls.value.clone(),
    }
}
