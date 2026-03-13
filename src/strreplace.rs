//! strreplace.rs — AI-driven surgical file edits.
//!
//! Replaces the unified-diff workflow.  The AI returns ```strreplace blocks
//! containing a FILE / FIND / REPLACE directive.  We locate the file, find
//! the exact text (or a whitespace-normalised fuzzy match), replace it, and
//! write the result back to disk.
//!
//! One `StrReplaceOp` per change location.  Multiple ops in one block are
//! separated by a `---` line.

use std::path::{Path, PathBuf};

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StrReplaceOp {
    pub file:    String,   // as given by the AI (filename or relative path)
    pub find:    String,   // exact text to locate
    pub replace: String,   // replacement text
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a strreplace code block into one or more ops.
///
/// Format (one op):
/// ```
/// FILE: canvas.rs
/// FIND:
/// exact lines to replace
/// REPLACE:
/// new lines
/// ```
///
/// Multiple ops in one block are separated by a bare `---` line.
pub fn parse_strreplace_block(content: &str) -> Vec<StrReplaceOp> {
    let mut ops   = Vec::new();
    let mut file    = String::new();
    let mut find    = String::new();
    let mut replace = String::new();

    #[derive(PartialEq)]
    enum State { None, Find, Replace }
    let mut state = State::None;

    for line in content.lines() {
        // Separator between multiple ops
        if line.trim() == "---" {
            flush(&mut ops, &mut file, &mut find, &mut replace);
            state = State::None;
            continue;
        }

        // FILE: directive (also acts as separator / start of new op)
        if let Some(rest) = line.strip_prefix("FILE:") {
            flush(&mut ops, &mut file, &mut find, &mut replace);
            file  = rest.trim().to_string();
            state = State::None;
            continue;
        }

        if line.trim() == "FIND:" {
            state = State::Find;
            continue;
        }
        if line.trim() == "REPLACE:" {
            state = State::Replace;
            continue;
        }

        match state {
            State::Find => {
                if !find.is_empty() { find.push('\n'); }
                find.push_str(line);
            }
            State::Replace => {
                if !replace.is_empty() { replace.push('\n'); }
                replace.push_str(line);
            }
            State::None => {}
        }
    }

    flush(&mut ops, &mut file, &mut find, &mut replace);
    ops
}

fn flush(ops: &mut Vec<StrReplaceOp>, file: &mut String, find: &mut String, replace: &mut String) {
    if !file.is_empty() && !find.is_empty() {
        ops.push(StrReplaceOp {
            file:    file.clone(),
            find:    find.clone(),
            replace: replace.clone(),
        });
    }
    find.clear();
    replace.clear();
}

/// Return a short display name from the first op's file field.
pub fn display_name(content: &str) -> String {
    let ops = parse_strreplace_block(content);
    if ops.is_empty() {
        return "file".to_string();
    }
    let files: Vec<String> = ops.iter()
        .map(|op| Path::new(&op.file).file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&op.file)
            .to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    files.join(", ")
}

// ── Applier ───────────────────────────────────────────────────────────────────

/// Apply all ops in a strreplace block to files on disk.
///
/// Ops that target the same file are applied sequentially so each one
/// sees the result of the previous.  Returns a human-readable summary
/// or an error message.
pub fn apply_strreplace_block(content: &str, base_dir: &Path) -> Result<String, String> {
    let ops = parse_strreplace_block(content);
    if ops.is_empty() {
        return Err("No valid FILE/FIND/REPLACE blocks found.".to_string());
    }
    apply_ops(&ops, base_dir)
}

pub fn apply_ops(ops: &[StrReplaceOp], base_dir: &Path) -> Result<String, String> {
    // Group ops by resolved path, preserving order
    let mut file_ops: Vec<(PathBuf, Vec<&StrReplaceOp>)> = Vec::new();
    for op in ops {
        let path = resolve_file(&op.file, base_dir)
            .ok_or_else(|| format!("File not found: '{}' (searched from {})", op.file, base_dir.display()))?;
        if let Some(entry) = file_ops.iter_mut().find(|(p, _)| p == &path) {
            entry.1.push(op);
        } else {
            file_ops.push((path, vec![op]));
        }
    }

    let mut total_applied = 0usize;
    for (path, file_ops_list) in &file_ops {
        let original = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;

        let mut content = original.clone();
        for op in file_ops_list {
            content = apply_single_op(&content, op, path)?;
            total_applied += 1;
        }

        // Preserve trailing newline behaviour of original
        let content = if original.ends_with('\n') && !content.ends_with('\n') {
            content + "\n"
        } else {
            content
        };

        std::fs::write(path, &content)
            .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
    }

    let file_count = file_ops.len();
    Ok(format!("Applied {} change(s) to {} file(s)", total_applied, file_count))
}

fn apply_single_op(content: &str, op: &StrReplaceOp, path: &Path) -> Result<String, String> {
    // Strategy 1: exact match
    if content.contains(&op.find) {
        return Ok(content.replacen(&op.find, &op.replace, 1));
    }

    // Strategy 2: trailing-whitespace normalised (trim_end per line)
    let norm_content = normalize_trailing(content);
    let norm_find    = normalize_trailing(&op.find);
    if norm_content.contains(&norm_find) {
        // Find the span in the original file and replace it
        if let Some(result) = replace_normalised(content, &op.find, &op.replace) {
            return Ok(result);
        }
    }

    // Strategy 3: fully trimmed per line (handles mixed indentation)
    if let Some(result) = replace_fuzzy_trimmed(content, &op.find, &op.replace) {
        return Ok(result);
    }

    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    Err(format!(
        "Could not find the target code in {}.\n\
         First FIND line: {:?}\n\
         Tip: make sure FIND lines are copied verbatim from the file.",
        fname,
        op.find.lines().next().unwrap_or("(empty)")
    ))
}

// ── Fuzzy matching helpers ─────────────────────────────────────────────────────

fn normalize_trailing(s: &str) -> String {
    s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

/// Find the find-lines in content by line matching with trim_end, then splice
/// in the replacement preserving the original surrounding text.
fn replace_normalised(content: &str, find: &str, replace: &str) -> Option<String> {
    let find_lines: Vec<&str> = find.lines().collect();
    let cont_lines: Vec<&str> = content.lines().collect();
    let n = find_lines.len();

    'outer: for i in 0..=cont_lines.len().saturating_sub(n) {
        for (j, fl) in find_lines.iter().enumerate() {
            if cont_lines[i + j].trim_end() != fl.trim_end() {
                continue 'outer;
            }
        }
        // Match at line i..i+n
        let before: Vec<&str> = cont_lines[..i].to_vec();
        let after:  Vec<&str> = cont_lines[i + n..].to_vec();
        let mut result = before.join("\n");
        if !result.is_empty() { result.push('\n'); }
        result.push_str(replace);
        if !after.is_empty() {
            result.push('\n');
            result.push_str(&after.join("\n"));
        }
        return Some(result);
    }
    None
}

/// Fully trimmed per-line match (handles indentation differences).
fn replace_fuzzy_trimmed(content: &str, find: &str, replace: &str) -> Option<String> {
    let find_lines: Vec<&str> = find.lines().collect();
    // Only use this strategy if lines are at least 8 chars (avoids false positives on `}`)
    if find_lines.iter().all(|l| l.trim().len() < 8) {
        return None;
    }
    let cont_lines: Vec<&str> = content.lines().collect();
    let n = find_lines.len();

    'outer: for i in 0..=cont_lines.len().saturating_sub(n) {
        for (j, fl) in find_lines.iter().enumerate() {
            if cont_lines[i + j].trim() != fl.trim() {
                continue 'outer;
            }
        }
        let before: Vec<&str> = cont_lines[..i].to_vec();
        let after:  Vec<&str> = cont_lines[i + n..].to_vec();
        let mut result = before.join("\n");
        if !result.is_empty() { result.push('\n'); }
        result.push_str(replace);
        if !after.is_empty() {
            result.push('\n');
            result.push_str(&after.join("\n"));
        }
        return Some(result);
    }
    None
}

// ── File resolver ─────────────────────────────────────────────────────────────

fn resolve_file(name: &str, base_dir: &Path) -> Option<PathBuf> {
    let p = Path::new(name);

    // Absolute path
    if p.is_absolute() && p.exists() { return Some(p.to_path_buf()); }

    // Relative to base_dir
    let candidate = base_dir.join(name);
    if candidate.exists() { return Some(candidate); }

    // From git root
    if let Some(root) = find_git_root(base_dir) {
        let candidate = root.join(name);
        if candidate.exists() { return Some(candidate); }

        // Search by filename under git root (max depth 6)
        if let Some(filename) = p.file_name() {
            if let Some(found) = search_by_name(&root, filename, 6) {
                return Some(found);
            }
        }
    }

    // Search by filename under base_dir
    if let Some(filename) = p.file_name() {
        if let Some(found) = search_by_name(base_dir, filename, 4) {
            return Some(found);
        }
    }

    None
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join(".git").exists() { return Some(dir); }
        if !dir.pop() { return None; }
    }
}

fn search_by_name(dir: &Path, name: &std::ffi::OsStr, max_depth: usize) -> Option<PathBuf> {
    if max_depth == 0 { return None; }
    let rd = std::fs::read_dir(dir).ok()?;
    let mut found: Option<PathBuf> = None;
    for entry in rd.flatten() {
        let path = entry.path();
        let ft = entry.file_type().ok()?;
        if ft.is_dir() {
            // Skip hidden and build dirs
            let dname = entry.file_name();
            let s = dname.to_string_lossy();
            if s.starts_with('.') || s == "target" || s == "node_modules" {
                continue;
            }
            if let Some(f) = search_by_name(&path, name, max_depth - 1) {
                // If we already found one, ambiguous — prefer src/ paths
                match &found {
                    None => found = Some(f),
                    Some(existing) => {
                        // prefer src/ over other dirs
                        if f.to_string_lossy().contains("/src/") {
                            found = Some(f);
                        } else {
                            let _ = existing; // keep first
                        }
                    }
                }
            }
        } else if ft.is_file() && entry.file_name() == name {
            if found.is_none() {
                found = Some(path);
            }
        }
    }
    found
}
