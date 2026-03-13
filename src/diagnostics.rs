//! diagnostics.rs — Inline diagnostic marks from `cargo check` / `cargo clippy`.
//!
//! Parses `cargo --message-format=json` stdout lines into structured [`Diagnostic`]
//! items, then applies TextTag underlines to open editor buffers.

use std::path::{Path, PathBuf};
use gtk4::prelude::*;
use gtk4::TextTag;
use sourceview5::Buffer;
use crate::tab::EditorTab;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum DiagLevel {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Absolute (or project-relative) path to the affected file.
    pub file:       PathBuf,
    /// 1-based line/column of the primary span start/end.
    pub line_start: u32,
    pub col_start:  u32,
    pub line_end:   u32,
    pub col_end:    u32,
    pub level:      DiagLevel,
    /// Short message text (e.g. "unused variable `x`").
    pub message:    String,
    /// Full human-readable text rendered by rustc (used in Problems tab).
    pub rendered:   String,
}

// ── JSON parsing ──────────────────────────────────────────────────────────────

/// Parse raw `cargo --message-format=json` stdout lines into Diagnostics.
/// `project_dir` is used to resolve relative file paths in spans.
pub fn parse_cargo_json(lines: &[String], project_dir: &Path) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for line in lines {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if val["reason"].as_str() != Some("compiler-message") { continue }

        let msg      = &val["message"];
        let level    = match msg["level"].as_str().unwrap_or("note") {
            "error"   => DiagLevel::Error,
            "warning" => DiagLevel::Warning,
            _         => DiagLevel::Note,
        };
        let message  = msg["message"].as_str().unwrap_or("").to_string();
        let rendered = msg["rendered"].as_str().unwrap_or("").to_string();

        let Some(spans) = msg["spans"].as_array() else { continue };
        for span in spans {
            if span["is_primary"].as_bool() != Some(true) { continue }

            let file_name = span["file_name"].as_str().unwrap_or("");
            let file = if Path::new(file_name).is_absolute() {
                PathBuf::from(file_name)
            } else {
                project_dir.join(file_name)
            };

            result.push(Diagnostic {
                file,
                line_start: span["line_start"].as_u64().unwrap_or(1) as u32,
                line_end:   span["line_end"].as_u64().unwrap_or(1) as u32,
                col_start:  span["column_start"].as_u64().unwrap_or(1) as u32,
                col_end:    span["column_end"].as_u64().unwrap_or(1) as u32,
                level: level.clone(),
                message: message.clone(),
                rendered: rendered.clone(),
            });
        }
    }
    result
}

// ── Inline marks ──────────────────────────────────────────────────────────────

const TAG_ERROR:   &str = "diag-error";
const TAG_WARNING: &str = "diag-warning";

fn ensure_tags(buf: &Buffer) {
    let table = buf.tag_table();
    if table.lookup(TAG_ERROR).is_none() {
        let t = TextTag::builder()
            .name(TAG_ERROR)
            .underline(gtk4::pango::Underline::Error)
            .build();
        table.add(&t);
    }
    if table.lookup(TAG_WARNING).is_none() {
        let t = TextTag::builder()
            .name(TAG_WARNING)
            .underline(gtk4::pango::Underline::Single)
            .foreground("#f9e2af")
            .build();
        table.add(&t);
    }
}

/// Remove all diagnostic underline marks from a tab's buffer.
pub fn clear_tab(tab: &EditorTab) {
    let buf = tab.buffer();
    let (start, end) = buf.bounds();
    let table = buf.tag_table();
    if let Some(t) = table.lookup(TAG_ERROR)   { buf.remove_tag(&t, &start, &end); }
    if let Some(t) = table.lookup(TAG_WARNING) { buf.remove_tag(&t, &start, &end); }
}

/// Apply diagnostics as inline underline marks on the given tab.
/// Clears any existing marks first.
pub fn apply_to_tab(tab: &EditorTab, diags: &[Diagnostic]) {
    let Some(tab_path) = tab.path() else { return };
    let buf = tab.buffer();
    ensure_tags(&buf);
    clear_tab(tab);

    let tab_canon = std::fs::canonicalize(&tab_path).unwrap_or(tab_path.clone());

    for diag in diags {
        let diag_canon = std::fs::canonicalize(&diag.file).unwrap_or_else(|_| diag.file.clone());
        if diag_canon != tab_canon { continue }

        let tag_name = match diag.level {
            DiagLevel::Error   => TAG_ERROR,
            DiagLevel::Warning => TAG_WARNING,
            DiagLevel::Note    => continue,
        };
        let table = buf.tag_table();
        let Some(tag) = table.lookup(tag_name) else { continue };

        // TextBuffer uses 0-based lines; cargo spans are 1-based.
        let l0 = diag.line_start.saturating_sub(1) as i32;
        let l1 = diag.line_end.saturating_sub(1)   as i32;
        let c0 = diag.col_start.saturating_sub(1)  as i32;
        let c1 = diag.col_end.saturating_sub(1)    as i32;

        let Some(mut s) = buf.iter_at_line(l0) else { continue };
        let slen = s.chars_in_line().saturating_sub(1).max(0);
        s.set_line_offset(c0.min(slen));

        let Some(mut e) = buf.iter_at_line(l1) else { continue };
        let elen = e.chars_in_line().saturating_sub(1).max(0);
        e.set_line_offset(c1.min(elen));

        if e <= s {
            e = s.clone();
            if !e.ends_line() { e.forward_word_end(); }
        }

        buf.apply_tag(&tag, &s, &e);
    }
}
