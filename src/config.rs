//! Standalone configuration for Rui.
//!
//! Replaces the rdm-common dependency. Reads ~/.config/rui/rui.toml.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Editor configuration — same fields as the original EditorConfig.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EditorConfig {
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: u32,
    #[serde(default = "default_true")]
    pub insert_spaces: bool,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default = "default_true")]
    pub highlight_current_line: bool,
    #[serde(default)]
    pub word_wrap: bool,
    #[serde(default = "default_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_true")]
    pub show_sidebar: bool,
    #[serde(default)]
    pub show_output: bool,
    #[serde(default)]
    pub default_dir: String,
    #[serde(default)]
    pub autosave: bool,
    #[serde(default = "default_true")]
    pub show_preview: bool,
}

fn default_font() -> String { "JetBrains Mono 13".into() }
fn default_tab_width() -> u32 { 4 }
fn default_scheme() -> String { String::new() }
fn default_true() -> bool { true }

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            font: default_font(),
            tab_width: default_tab_width(),
            insert_spaces: true,
            show_line_numbers: true,
            highlight_current_line: true,
            word_wrap: false,
            color_scheme: default_scheme(),
            show_sidebar: true,
            show_output: false,
            default_dir: String::new(),
            autosave: false,
            show_preview: true,
        }
    }
}

/// Top-level config file structure (rui.toml).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct RuiConfig {
    #[serde(default)]
    editor: EditorConfig,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("rui")
        .join("rui.toml")
}

/// Load editor configuration from ~/.config/rui/rui.toml, with defaults.
pub fn load() -> EditorConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let cfg: RuiConfig = toml::from_str(&text).unwrap_or_default();
            cfg.editor
        }
        Err(_) => EditorConfig::default(),
    }
}

/// Return the startup directory: configured default_dir → home → /tmp.
pub fn startup_dir(cfg: &EditorConfig) -> PathBuf {
    if !cfg.default_dir.is_empty() {
        let p = PathBuf::from(&cfg.default_dir);
        if p.is_dir() {
            return p;
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Embedded structural CSS for the editor UI.
/// Colours are intentionally omitted so Rui follows the active GTK theme.
pub fn default_css() -> String {
    r#"
/* Rui structural styles — no hardcoded colours */
.editor-tab-label {
    padding: 4px 8px;
}
.editor-tab-close {
    padding: 0 4px;
    min-width: 16px;
    min-height: 16px;
    border: none;
    background: transparent;
}
.editor-tab-modified {
    font-style: italic;
}
.editor-statusbar {
    padding: 2px 8px;
}
.editor-statusbar-item {
    padding: 0 8px;
    font-size: 0.9em;
}
.editor-statusbar-modified {
    font-weight: bold;
}
.editor-output {
    font-family: monospace;
    padding: 4px 8px;
}
.editor-help-title {
    font-size: 1.4em;
    font-weight: bold;
}
"#.to_string()
}
