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
    apply_btn.set_tooltip_text(Some("Apply the diff using git apply (runs in the git root)"));

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

    // If a diff was pre-loaded, populate and auto-extract.
    if let Some(diff) = preloaded_diff {
        paste_buf.set_text(diff);
        let extracted = extract_diff(diff);
        if !extracted.is_empty() {
            populate_preview(&preview_buf, &extracted);
            paste_buf.set_text(&extracted);
            status_lbl.set_markup(&format!(
                "<span foreground='#a6e3a1'>Diff loaded — {} lines. Review and click Apply.</span>",
                extracted.lines().count()
            ));
            apply_btn.set_sensitive(true);
        }
    }

    win.present();
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
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let check = std::process::Command::new("git")
        .args(["apply", "--check", tmp.to_str().unwrap_or("")])
        .current_dir(&git_root)
        .output()
        .map_err(|e| format!("Could not run git: {}", e))?;

    if !check.status.success() {
        let stderr = String::from_utf8_lossy(&check.stderr).to_string();
        let patch = std::process::Command::new("patch")
            .args(["-p1", "--dry-run", "-i", tmp.to_str().unwrap_or("")])
            .current_dir(&git_root)
            .output()
            .map_err(|e| format!("git apply failed and patch not available: {} — {}", stderr, e))?;
        if !patch.status.success() {
            return Err(format!(
                "git apply check failed: {}",
                stderr.trim()
            ));
        }
        let result = std::process::Command::new("patch")
            .args(["-p1", "-i", tmp.to_str().unwrap_or("")])
            .current_dir(&git_root)
            .output()
            .map_err(|e| format!("patch failed: {}", e))?;
        if result.status.success() {
            return Ok("Applied with patch(1) successfully.".to_string());
        } else {
            return Err(String::from_utf8_lossy(&result.stderr).to_string());
        }
    }

    let apply = std::process::Command::new("git")
        .args(["apply", tmp.to_str().unwrap_or("")])
        .current_dir(&git_root)
        .output()
        .map_err(|e| format!("Could not run git apply: {}", e))?;

    if apply.status.success() {
        Ok(format!("Applied successfully in {}", git_root.display()))
    } else {
        Err(String::from_utf8_lossy(&apply.stderr).to_string())
    }
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
