use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Label, Orientation, Paned,
    ScrolledWindow, TextView, TextBuffer, Window,
};
use std::path::PathBuf;

/// Open the diff dialog with a diff already loaded, extracted, and previewed.
pub fn show_diff_dialog_with_diff(
    parent: &gtk4::ApplicationWindow,
    working_dir: Option<PathBuf>,
    diff_text: &str,
) {
    show_diff_dialog_inner(parent, working_dir, Some(diff_text));
}

pub fn show_diff_dialog(parent: &gtk4::ApplicationWindow, working_dir: Option<PathBuf>) {
    show_diff_dialog_inner(parent, working_dir, None);
}

fn show_diff_dialog_inner(
    parent: &gtk4::ApplicationWindow,
    working_dir: Option<PathBuf>,
    preloaded_diff: Option<&str>,
) {
    let win = Window::builder()
        .title("Apply AI Diff")
        .transient_for(parent)
        .modal(false)
        .default_width(800)
        .default_height(680)
        .resizable(true)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);

    let top_bar = GtkBox::new(Orientation::Horizontal, 8);
    top_bar.set_margin_start(12);
    top_bar.set_margin_end(12);
    top_bar.set_margin_top(10);
    top_bar.set_margin_bottom(6);

    let instructions = Label::new(Some(
        "Paste the AI's response below. Click \"Extract & Preview\" to isolate the diff, then \"Apply\".",
    ));
    instructions.set_halign(Align::Start);
    instructions.set_hexpand(true);
    instructions.set_wrap(true);
    instructions.add_css_class("dim-label");

    top_bar.append(&instructions);
    root.append(&top_bar);

    let paned = Paned::new(Orientation::Vertical);
    paned.set_vexpand(true);

    let paste_buf = TextBuffer::new(None);
    let paste_view = TextView::with_buffer(&paste_buf);
    paste_view.set_monospace(true);
    paste_view.set_editable(true);
    paste_view.set_wrap_mode(gtk4::WrapMode::None);

    let paste_scroll = ScrolledWindow::builder()
        .child(&paste_view)
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(4)
        .build();

    let paste_wrap = GtkBox::new(Orientation::Vertical, 4);
    let paste_lbl = Label::new(Some("Pasted AI Response"));
    paste_lbl.set_halign(Align::Start);
    paste_lbl.set_margin_start(12);
    paste_lbl.set_margin_top(2);
    paste_lbl.add_css_class("dim-label");
    paste_wrap.append(&paste_lbl);
    paste_wrap.append(&paste_scroll);

    let preview_buf = TextBuffer::new(None);
    setup_diff_tags(&preview_buf);

    let preview_view = TextView::with_buffer(&preview_buf);
    preview_view.set_monospace(true);
    preview_view.set_editable(false);
    preview_view.set_wrap_mode(gtk4::WrapMode::None);

    let preview_scroll = ScrolledWindow::builder()
        .child(&preview_view)
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(4)
        .build();

    let preview_wrap = GtkBox::new(Orientation::Vertical, 4);
    let preview_lbl = Label::new(Some("Diff Preview"));
    preview_lbl.set_halign(Align::Start);
    preview_lbl.set_margin_start(12);
    preview_lbl.set_margin_top(2);
    preview_lbl.add_css_class("dim-label");
    preview_wrap.append(&preview_lbl);
    preview_wrap.append(&preview_scroll);

    paned.set_start_child(Some(&paste_wrap));
    paned.set_end_child(Some(&preview_wrap));
    paned.set_position(280);
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);

    root.append(&paned);

    let status_lbl = Label::new(Some(""));
    status_lbl.set_halign(Align::Start);
    status_lbl.set_margin_start(12);
    status_lbl.set_margin_top(4);
    status_lbl.set_margin_bottom(4);
    root.append(&status_lbl);

    let btn_row = GtkBox::new(Orientation::Horizontal, 8);
    btn_row.set_margin_start(12);
    btn_row.set_margin_end(12);
    btn_row.set_margin_top(4);
    btn_row.set_margin_bottom(10);

    let extract_btn = Button::with_label("Extract & Preview");
    extract_btn.set_tooltip_text(Some("Find the unified diff block in the pasted text and highlight it"));

    let apply_btn = Button::with_label("Apply Diff");
    apply_btn.add_css_class("suggested-action");
    apply_btn.set_sensitive(false);
    apply_btn.set_tooltip_text(Some("Apply the diff (tries git apply, patch, then manual fallback)"));

    let close_btn = Button::with_label("Close");

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    btn_row.append(&extract_btn);
    btn_row.append(&apply_btn);
    btn_row.append(&spacer);
    btn_row.append(&close_btn);
    root.append(&btn_row);

    win.set_child(Some(&root));

    // Extract & Preview
    {
        let paste_buf_c  = paste_buf.clone();
        let preview_buf_c = preview_buf.clone();
        let apply_btn_c  = apply_btn.clone();
        let status_c     = status_lbl.clone();

        extract_btn.connect_clicked(move |_| {
            let (start, end) = paste_buf_c.bounds();
            let raw = paste_buf_c.text(&start, &end, false).to_string();
            let diff = extract_diff(&raw);
            if diff.is_empty() {
                status_c.set_markup("<span foreground='#f38ba8'>No unified diff found in pasted text.</span>");
                apply_btn_c.set_sensitive(false);
            } else {
                populate_preview(&preview_buf_c, &diff);
                paste_buf_c.set_text(&diff);
                status_c.set_markup(&format!(
                    "<span foreground='#a6e3a1'>Diff extracted — {} lines.</span>",
                    diff.lines().count()
                ));
                apply_btn_c.set_sensitive(true);
            }
        });
    }

    // Apply
    {
        let paste_buf_c = paste_buf.clone();
        let status_c    = status_lbl.clone();
        let wd          = working_dir.clone();

        apply_btn.connect_clicked(move |_| {
            let (start, end) = paste_buf_c.bounds();
            let diff_text = paste_buf_c.text(&start, &end, false).to_string();
            if diff_text.trim().is_empty() {
                status_c.set_markup("<span foreground='#f38ba8'>Nothing to apply.</span>");
                return;
            }
            match apply_diff(&diff_text, wd.as_deref()) {
                Ok(msg) => status_c.set_markup(&format!(
                    "<span foreground='#a6e3a1'>✓ {}</span>", glib_escape(&msg)
                )),
                Err(e) => status_c.set_markup(&format!(
                    "<span foreground='#f38ba8'>✗ {}</span>", glib_escape(&e)
                )),
            }
        });
    }

    // Close
    {
        let win_c = win.clone();
        close_btn.connect_clicked(move |_| win_c.close());
    }

    let key_ctrl = gtk4::EventControllerKey::new();
    let win_c = win.clone();
    key_ctrl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            win_c.close();
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    win.add_controller(key_ctrl);

    // If a diff was pre-loaded, skip extract_diff (text is already a clean diff
    // from a ```diff code block) — just normalize and preview directly.
    if let Some(diff) = preloaded_diff {
        let cleaned = normalize_diff(diff);
        if !cleaned.is_empty() {
            paste_buf.set_text(&cleaned);
            populate_preview(&preview_buf, &cleaned);
            status_lbl.set_markup(&format!(
                "<span foreground='#a6e3a1'>Diff loaded — {} lines. Review and click Apply.</span>",
                cleaned.lines().count()
            ));
            apply_btn.set_sensitive(true);
        } else {
            paste_buf.set_text(diff);
            status_lbl.set_markup(
                "<span foreground='#f38ba8'>Could not parse diff. You can edit it above, then Extract &amp; Preview.</span>"
            );
        }
    }

    win.present();
}

/// Normalize a diff that's already been extracted from a code block.
/// Ensures it has proper --- / +++ headers and @@ hunks.
fn normalize_diff(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() { return String::new(); }

    // Check if it already looks like a valid diff
    let has_header = text.lines().any(|l| l.starts_with("--- "));
    let has_hunk   = text.lines().any(|l| l.starts_with("@@ "));

    if has_header && has_hunk {
        // Already a proper diff — just ensure trailing newline
        return format!("{}\n", text);
    }

    // If it has hunks but no header, it's malformed — can't fix that easily
    if has_hunk && !has_header {
        return format!("{}\n", text);
    }

    String::new()
}

fn extract_diff(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut start_idx: Option<usize> = None;
    let mut end_idx = 0usize;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if start_idx.is_none()
            && (line.starts_with("--- ")
                || line.starts_with("diff --git "))
        {
            start_idx = Some(i);
        }
        if start_idx.is_some() {
            if line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("@@ ")
                || line.starts_with('+')
                || line.starts_with('-')
                || line.starts_with(' ')
                || line.starts_with("diff --git")
                || line.starts_with("index ")
                || line.starts_with("\\ No newline")
            {
                end_idx = i;
            } else if !line.is_empty() {
                let had_hunk = lines[start_idx.unwrap()..=end_idx]
                    .iter()
                    .any(|l| l.starts_with("@@ "));
                if had_hunk {
                    break;
                }
                start_idx = None;
            }
        }
        i += 1;
    }

    match start_idx {
        Some(s) if end_idx >= s => {
            lines[s..=end_idx].join("\n") + "\n"
        }
        _ => String::new(),
    }
}

fn populate_preview(buf: &TextBuffer, diff: &str) {
    buf.set_text("");
    let mut end = buf.end_iter();
    for line in diff.lines() {
        let tag = if line.starts_with('+') && !line.starts_with("+++") {
            Some("diff-added")
        } else if line.starts_with('-') && !line.starts_with("---") {
            Some("diff-removed")
        } else if line.starts_with("@@") {
            Some("diff-hunk")
        } else if line.starts_with("---") || line.starts_with("+++") || line.starts_with("diff ") {
            Some("diff-header")
        } else {
            None
        };

        let line_with_nl = format!("{}\n", line);
        let insert_offset = end.offset();
        buf.insert(&mut end, &line_with_nl);

        if let Some(tag_name) = tag {
            let start = buf.iter_at_offset(insert_offset);
            let end2 = buf.end_iter();
            buf.apply_tag_by_name(tag_name, &start, &end2);
        }
        end = buf.end_iter();
    }
}

fn setup_diff_tags(buf: &TextBuffer) {
    buf.create_tag(Some("diff-added"),   &[("foreground", &"#a6e3a1")]);
    buf.create_tag(Some("diff-removed"), &[("foreground", &"#f38ba8")]);
    buf.create_tag(Some("diff-hunk"),    &[("foreground", &"#89b4fa"), ("weight", &700i32)]);
    buf.create_tag(Some("diff-header"),  &[("foreground", &"#cba6f7"), ("weight", &700i32)]);
}

fn apply_diff(diff: &str, working_dir: Option<&std::path::Path>) -> Result<String, String> {
    let tmp = std::env::temp_dir().join("rui-ai.patch");
    std::fs::write(&tmp, diff)
        .map_err(|e| format!("Could not write patch file: {}", e))?;

    let git_root = working_dir
        .and_then(find_git_root)
        .unwrap_or_else(|| working_dir
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()));

    let tmp_str = tmp.to_str().unwrap_or("");

    // Strategy 1: git apply (strict)
    if try_git_apply(&git_root, tmp_str, &[]).is_ok() {
        return Ok(format!("Applied with git apply in {}", git_root.display()));
    }

    // Strategy 2: git apply --ignore-whitespace
    if try_git_apply(&git_root, tmp_str, &["--ignore-whitespace"]).is_ok() {
        return Ok(format!("Applied with git apply --ignore-whitespace in {}", git_root.display()));
    }

    // Strategy 3: git apply --3way --ignore-whitespace
    if try_git_apply(&git_root, tmp_str, &["--3way", "--ignore-whitespace"]).is_ok() {
        return Ok(format!("Applied with git apply --3way in {}", git_root.display()));
    }

    // Strategy 4: patch -p1 with fuzz
    if try_patch(&git_root, tmp_str, &["-p1", "--fuzz=3"]).is_ok() {
        return Ok("Applied with patch -p1 --fuzz=3".to_string());
    }

    // Strategy 5: patch -p0 with fuzz (for diffs without a/ b/ prefix)
    if try_patch(&git_root, tmp_str, &["-p0", "--fuzz=3"]).is_ok() {
        return Ok("Applied with patch -p0 --fuzz=3".to_string());
    }

    // Strategy 6: manual fuzzy apply
    match manual_apply(diff, &git_root) {
        Ok(msg) => return Ok(msg),
        Err(manual_err) => {
            // Collect the original git error for the message
            let git_err = std::process::Command::new("git")
                .args(["apply", "--check", "--ignore-whitespace", tmp_str])
                .current_dir(&git_root)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                .unwrap_or_default();
            return Err(format!(
                "All strategies failed.\ngit apply: {}\nmanual: {}\nTip: edit the diff above to fix context lines, then retry.",
                git_err.trim(), manual_err
            ));
        }
    }
}

fn try_git_apply(dir: &std::path::Path, patch_file: &str, extra_args: &[&str]) -> Result<(), ()> {
    // Check first
    let mut args = vec!["apply", "--check"];
    args.extend_from_slice(extra_args);
    args.push(patch_file);
    let check = std::process::Command::new("git")
        .args(&args)
        .current_dir(dir)
        .output()
        .map_err(|_| ())?;
    if !check.status.success() { return Err(()); }

    // Actually apply
    let mut args = vec!["apply"];
    args.extend_from_slice(extra_args);
    args.push(patch_file);
    let apply = std::process::Command::new("git")
        .args(&args)
        .current_dir(dir)
        .output()
        .map_err(|_| ())?;
    if apply.status.success() { Ok(()) } else { Err(()) }
}

fn try_patch(dir: &std::path::Path, patch_file: &str, args: &[&str]) -> Result<(), ()> {
    // Dry run first
    let mut dry_args: Vec<&str> = args.to_vec();
    dry_args.extend_from_slice(&["--dry-run", "-i", patch_file]);
    let check = std::process::Command::new("patch")
        .args(&dry_args)
        .current_dir(dir)
        .output()
        .map_err(|_| ())?;
    if !check.status.success() { return Err(()); }

    // Actually apply
    let mut real_args: Vec<&str> = args.to_vec();
    real_args.extend_from_slice(&["-i", patch_file]);
    let apply = std::process::Command::new("patch")
        .args(&real_args)
        .current_dir(dir)
        .output()
        .map_err(|_| ())?;
    if apply.status.success() { Ok(()) } else { Err(()) }
}

// ── Manual fuzzy diff apply ──────────────────────────────────────────────────

/// Parse a unified diff and apply it by fuzzy-matching context lines in the file.
fn manual_apply(diff: &str, base_dir: &std::path::Path) -> Result<String, String> {
    let hunks = parse_diff_hunks(diff)?;
    if hunks.is_empty() {
        return Err("No hunks found in diff".into());
    }

    // Group hunks by target file
    let mut by_file: std::collections::HashMap<String, Vec<&DiffHunk>> = std::collections::HashMap::new();
    for hunk in &hunks {
        by_file.entry(hunk.file.clone()).or_default().push(hunk);
    }

    let mut applied_count = 0;
    for (file_path, file_hunks) in &by_file {
        let full_path = base_dir.join(file_path);
        if !full_path.exists() {
            return Err(format!("File not found: {} (looked in {})", file_path, base_dir.display()));
        }

        let original = std::fs::read_to_string(&full_path)
            .map_err(|e| format!("Could not read {}: {}", file_path, e))?;
        let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();

        // Apply hunks in reverse order so line offsets don't shift
        let mut sorted_hunks: Vec<&&DiffHunk> = file_hunks.iter().collect();
        sorted_hunks.sort_by(|a, b| b.old_start.cmp(&a.old_start));

        for hunk in sorted_hunks {
            lines = apply_hunk(&lines, hunk)?;
            applied_count += 1;
        }

        let result = lines.join("\n");
        // Preserve trailing newline if original had one
        let result = if original.ends_with('\n') && !result.ends_with('\n') {
            result + "\n"
        } else {
            result
        };
        std::fs::write(&full_path, &result)
            .map_err(|e| format!("Could not write {}: {}", file_path, e))?;
    }

    Ok(format!("Applied {} hunk(s) manually to {} file(s)", applied_count, by_file.len()))
}

enum DiffOp {
    Context(String),
    Remove(String),
    Add(String),
}

struct DiffHunk {
    file:      String,
    old_start: usize,
    ops:       Vec<DiffOp>,
}

fn parse_diff_hunks(diff: &str) -> Result<Vec<DiffHunk>, String> {
    let lines: Vec<&str> = diff.lines().collect();
    let mut hunks = Vec::new();
    let mut current_file = String::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Track the target file from +++ header
        if let Some(path) = line.strip_prefix("+++ ") {
            current_file = path
                .strip_prefix("b/").unwrap_or(path)
                .to_string();
            i += 1;
            continue;
        }

        // Parse @@ header
        if line.starts_with("@@ ") {
            if current_file.is_empty() {
                i += 1;
                continue;
            }
            let old_start = parse_hunk_header(line).unwrap_or(1);
            i += 1;

            // Collect hunk body preserving op order
            let mut ops = Vec::new();

            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("@@ ") || l.starts_with("--- ") || l.starts_with("+++ ") || l.starts_with("diff ") {
                    break;
                }
                if let Some(rem) = l.strip_prefix('-') {
                    ops.push(DiffOp::Remove(rem.to_string()));
                } else if let Some(add) = l.strip_prefix('+') {
                    ops.push(DiffOp::Add(add.to_string()));
                } else if let Some(ctx) = l.strip_prefix(' ') {
                    ops.push(DiffOp::Context(ctx.to_string()));
                } else if l == "\\ No newline at end of file" {
                    // skip
                } else {
                    // Unrecognized line — might be context without leading space
                    // (common AI mistake). Treat as context.
                    ops.push(DiffOp::Context(l.to_string()));
                }
                i += 1;
            }

            hunks.push(DiffHunk {
                file: current_file.clone(),
                old_start,
                ops,
            });
            continue;
        }

        i += 1;
    }

    Ok(hunks)
}

fn parse_hunk_header(line: &str) -> Option<usize> {
    // @@ -10,6 +10,7 @@ optional context
    let after_at = line.strip_prefix("@@ -")?;
    let comma_or_space = after_at.find(|c: char| c == ',' || c == ' ')?;
    after_at[..comma_or_space].parse::<usize>().ok()
}

fn apply_hunk(lines: &[String], hunk: &DiffHunk) -> Result<Vec<String>, String> {
    // "old" side = Context + Remove lines in order (what we search for in the file)
    let old: Vec<String> = hunk.ops.iter().filter_map(|op| match op {
        DiffOp::Context(s) | DiffOp::Remove(s) => Some(s.clone()),
        DiffOp::Add(_) => None,
    }).collect();

    // "new" side = Context + Add lines in order (the replacement)
    let new: Vec<String> = hunk.ops.iter().filter_map(|op| match op {
        DiffOp::Context(s) | DiffOp::Add(s) => Some(s.clone()),
        DiffOp::Remove(_) => None,
    }).collect();

    if old.is_empty() && new.is_empty() {
        return Ok(lines.to_vec());
    }

    let search_start = if hunk.old_start > 0 { hunk.old_start - 1 } else { 0 };
    let match_pos = match find_context(lines, &old, search_start) {
        Some(pos) => pos,
        None => {
            let first = old.first().map(|s| s.as_str()).unwrap_or("(empty)");
            return Err(format!(
                "Could not find matching context near line {} — first context line: \"{}\"",
                hunk.old_start, first
            ));
        }
    };

    let mut result = Vec::with_capacity(lines.len() + new.len());
    result.extend_from_slice(&lines[..match_pos]);
    result.extend(new);
    result.extend_from_slice(&lines[match_pos + old.len()..]);

    Ok(result)
}

/// Find the position in `lines` where `context` best matches, starting near `hint`.
/// Uses fuzzy matching: tries exact first, then trims whitespace.
fn find_context(lines: &[String], context: &[String], hint: usize) -> Option<usize> {
    if context.is_empty() { return Some(hint.min(lines.len())); }
    if context.len() > lines.len() { return None; }

    let max_pos = lines.len() - context.len();

    // Try exact match near hint first, then expand outward
    let hint = hint.min(max_pos);
    for offset in 0..=max_pos {
        for &dir in &[0i32, 1, -1] {
            let pos = if dir == 0 {
                hint
            } else if dir > 0 {
                if hint + offset <= max_pos { hint + offset } else { continue }
            } else {
                if offset <= hint { hint - offset } else { continue }
            };
            if pos > max_pos { continue; }

            if context_matches_at(lines, context, pos) {
                return Some(pos);
            }
        }
    }

    // Fuzzy: try trimmed whitespace comparison
    for pos in 0..=max_pos {
        if context_matches_fuzzy(lines, context, pos) {
            return Some(pos);
        }
    }

    None
}

fn context_matches_at(lines: &[String], context: &[String], pos: usize) -> bool {
    context.iter().enumerate().all(|(i, ctx)| {
        pos + i < lines.len() && lines[pos + i] == *ctx
    })
}

fn context_matches_fuzzy(lines: &[String], context: &[String], pos: usize) -> bool {
    context.iter().enumerate().all(|(i, ctx)| {
        pos + i < lines.len() && lines[pos + i].trim() == ctx.trim()
    })
}

fn find_git_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}
