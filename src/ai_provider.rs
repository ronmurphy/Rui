//! ai_provider.rs — Multi-provider API chat backend.
//!
//! Supports OpenAI, Anthropic (direct API), Google Gemini, and any
//! OpenAI-compatible endpoint (Ollama, Groq, Mistral, etc.).
//!
//! Each provider streams its response via `StreamEvent` through an
//! `async_channel::Sender` so the GTK main loop can update the UI
//! without blocking.  Runs on a background thread; no async runtime needed.

use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};

// ── Public types ──────────────────────────────────────────────────────────────

/// Which AI service to use.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    #[default]
    OpenAi,
    Anthropic,
    Gemini,
    /// Any OpenAI-compatible endpoint (Ollama, Groq, Mistral, …)
    OpenAiCompat,
}

impl AiProvider {
    pub fn label(&self) -> &'static str {
        match self {
            AiProvider::OpenAi       => "OpenAI",
            AiProvider::Anthropic    => "Anthropic",
            AiProvider::Gemini       => "Gemini",
            AiProvider::OpenAiCompat => "OpenAI-Compatible",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            AiProvider::OpenAi       => "gpt-4o",
            AiProvider::Anthropic    => "claude-3-5-sonnet-20241022",
            AiProvider::Gemini       => "gemini-2.0-flash",
            AiProvider::OpenAiCompat => "llama3",
        }
    }

    pub fn all() -> &'static [AiProvider] {
        &[
            AiProvider::OpenAi,
            AiProvider::Anthropic,
            AiProvider::Gemini,
            AiProvider::OpenAiCompat,
        ]
    }
}

/// Configuration for the AI panel (persisted in rui.toml).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub provider: AiProvider,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// Base URL for OpenAI-compat endpoints (e.g. http://localhost:11434/v1)
    #[serde(default)]
    pub base_url: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        let provider = AiProvider::default();
        let model = provider.default_model().to_string();
        AiConfig {
            provider,
            api_key: String::new(),
            model,
            base_url: String::new(),
        }
    }
}

/// Events streamed from the background thread to the GTK main loop.
pub enum StreamEvent {
    Text(String),
    Error(String),
    Done,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Spawn a streaming chat request on the current thread (call from a background thread).
///
/// `messages`: vec of `(role, content)` pairs — role is "user" or "assistant".
/// `system_prompt`: injected as the system message.
pub fn stream_chat(
    cfg: &AiConfig,
    messages: &[(String, String)],
    system_prompt: &str,
    tx: &async_channel::Sender<StreamEvent>,
    _proc_handle: &Arc<Mutex<Option<u32>>>,
) -> Result<(), String> {
    match cfg.provider {
        AiProvider::OpenAi | AiProvider::OpenAiCompat => {
            stream_openai(cfg, messages, system_prompt, tx, false)
        }
        AiProvider::Anthropic => {
            stream_anthropic(cfg, messages, system_prompt, tx)
        }
        AiProvider::Gemini => {
            stream_gemini(cfg, messages, system_prompt, tx)
        }
    }
}

// ── OpenAI / OpenAI-compat ────────────────────────────────────────────────────

fn stream_openai(
    cfg: &AiConfig,
    messages: &[(String, String)],
    system_prompt: &str,
    tx: &async_channel::Sender<StreamEvent>,
    _compat: bool,
) -> Result<(), String> {
    let base = if cfg.provider == AiProvider::OpenAiCompat && !cfg.base_url.is_empty() {
        cfg.base_url.trim_end_matches('/').to_string()
    } else {
        "https://api.openai.com/v1".to_string()
    };

    let url = format!("{}/chat/completions", base);
    let model = if cfg.model.is_empty() {
        cfg.provider.default_model().to_string()
    } else {
        cfg.model.clone()
    };

    // Build JSON messages array.
    let mut json_msgs = vec![
        serde_json::json!({"role": "system", "content": system_prompt})
    ];
    for (role, content) in messages {
        json_msgs.push(serde_json::json!({"role": role, "content": content}));
    }

    let body = serde_json::json!({
        "model": model,
        "messages": json_msgs,
        "stream": true,
    });

    let body_str = body.to_string();

    let mut req = ureq::post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream");

    if !cfg.api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", cfg.api_key));
    }

    let response = req.send_string(&body_str)
        .map_err(|e| format!("Request failed: {}", e))?;

    let reader = BufReader::new(response.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Read error: {}", e))?;
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        let data = line.strip_prefix("data: ").unwrap_or(&line);
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(text) = val
                .pointer("/choices/0/delta/content")
                .and_then(|v| v.as_str())
            {
                if !text.is_empty() {
                    let _ = tx.send_blocking(StreamEvent::Text(text.to_string()));
                }
            }
        }
    }
    Ok(())
}

// ── Anthropic ────────────────────────────────────────────────────────────────

fn stream_anthropic(
    cfg: &AiConfig,
    messages: &[(String, String)],
    system_prompt: &str,
    tx: &async_channel::Sender<StreamEvent>,
) -> Result<(), String> {
    let url = "https://api.anthropic.com/v1/messages";
    let model = if cfg.model.is_empty() {
        cfg.provider.default_model().to_string()
    } else {
        cfg.model.clone()
    };

    let mut json_msgs = Vec::new();
    for (role, content) in messages {
        json_msgs.push(serde_json::json!({"role": role, "content": content}));
    }

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": json_msgs,
        "stream": true,
    });

    let body_str = body.to_string();

    let response = ureq::post(url)
        .set("Content-Type", "application/json")
        .set("x-api-key", &cfg.api_key)
        .set("anthropic-version", "2023-06-01")
        .send_string(&body_str)
        .map_err(|e| format!("Request failed: {}", e))?;

    let reader = BufReader::new(response.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Read error: {}", e))?;
        if line.is_empty() { continue; }
        let data = line.strip_prefix("data: ").unwrap_or(&line);
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
            let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match event_type {
                "content_block_delta" => {
                    if let Some(text) = val.pointer("/delta/text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            let _ = tx.send_blocking(StreamEvent::Text(text.to_string()));
                        }
                    }
                }
                "message_delta" => {
                    // stop_reason etc — ignore
                }
                _ => {}
            }
        }
    }
    Ok(())
}

// ── Google Gemini ─────────────────────────────────────────────────────────────

fn stream_gemini(
    cfg: &AiConfig,
    messages: &[(String, String)],
    system_prompt: &str,
    tx: &async_channel::Sender<StreamEvent>,
) -> Result<(), String> {
    let model = if cfg.model.is_empty() {
        cfg.provider.default_model().to_string()
    } else {
        cfg.model.clone()
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        model, cfg.api_key
    );

    // Gemini uses "contents" with "parts"; system instruction is separate.
    let mut contents = Vec::new();
    for (role, content) in messages {
        let gemini_role = if role == "assistant" { "model" } else { "user" };
        contents.push(serde_json::json!({
            "role": gemini_role,
            "parts": [{"text": content}]
        }));
    }

    let body = serde_json::json!({
        "system_instruction": {
            "parts": [{"text": system_prompt}]
        },
        "contents": contents,
    });

    let body_str = body.to_string();

    let response = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_string(&body_str)
        .map_err(|e| format!("Request failed: {}", e))?;

    let reader = BufReader::new(response.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Read error: {}", e))?;
        if line.is_empty() { continue; }
        let data = line.strip_prefix("data: ").unwrap_or(&line);
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(text) = val
                .pointer("/candidates/0/content/parts/0/text")
                .and_then(|v| v.as_str())
            {
                if !text.is_empty() {
                    let _ = tx.send_blocking(StreamEvent::Text(text.to_string()));
                }
            }
        }
    }
    Ok(())
}
