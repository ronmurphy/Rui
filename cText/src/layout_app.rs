//! Handler stubs for layout.ui — cText simple notepad

#![allow(unused_imports)]
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{Builder, Button, Label, TextView};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub fn connect_handlers(builder: &Builder) {
    let btn_new: Button     = builder.object("btn_new").unwrap();
    let btn_open: Button    = builder.object("btn_open").unwrap();
    let btn_save: Button    = builder.object("btn_save").unwrap();
    let text_view: TextView = builder.object("text_view").unwrap();
    let lbl_filename: Label = builder.object("lbl_filename").unwrap();
    let lbl_status: Label   = builder.object("lbl_status").unwrap();

    let current_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let buffer = text_view.buffer();

    // ── New ─────────────────────────────────────────────────────────────────
    {
        let (buf, lbl_f, lbl_s, path) =
            (buffer.clone(), lbl_filename.clone(), lbl_status.clone(), current_path.clone());
        btn_new.connect_clicked(move |_| {
            buf.set_text("");
            lbl_f.set_label("Untitled");
            lbl_s.set_label("New file");
            *path.borrow_mut() = None;
        });
    }

    // ── Open ─────────────────────────────────────────────────────────────────
    {
        let (buf, lbl_f, lbl_s, path, tv) = (
            buffer.clone(), lbl_filename.clone(), lbl_status.clone(),
            current_path.clone(), text_view.clone(),
        );
        btn_open.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder().title("Open File").build();
            let (buf, lbl_f, lbl_s, path) =
                (buf.clone(), lbl_f.clone(), lbl_s.clone(), path.clone());
            let window = tv.root().and_downcast::<gtk4::Window>();
            dialog.open(window.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(p) = file.path() {
                        match std::fs::read_to_string(&p) {
                            Ok(contents) => {
                                buf.set_text(&contents);
                                lbl_f.set_label(
                                    p.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                                );
                                lbl_s.set_label(&format!("Opened: {}", p.display()));
                                *path.borrow_mut() = Some(p);
                            }
                            Err(e) => lbl_s.set_label(&format!("Open error: {e}")),
                        }
                    }
                }
            });
        });
    }

    // ── Save ─────────────────────────────────────────────────────────────────
    {
        let (buf, lbl_f, lbl_s, path, tv) = (
            buffer.clone(), lbl_filename.clone(), lbl_status.clone(),
            current_path.clone(), text_view.clone(),
        );
        btn_save.connect_clicked(move |_| {
            let maybe_path = path.borrow().clone();
            if let Some(p) = maybe_path {
                save_to_path(&buf, &p, &lbl_s);
            } else {
                // Save As
                let dialog = gtk4::FileDialog::builder().title("Save As").build();
                let (buf, lbl_f, lbl_s, path) =
                    (buf.clone(), lbl_f.clone(), lbl_s.clone(), path.clone());
                let window = tv.root().and_downcast::<gtk4::Window>();
                dialog.save(window.as_ref(), None::<&gio::Cancellable>, move |res| {
                    if let Ok(file) = res {
                        if let Some(p) = file.path() {
                            save_to_path(&buf, &p, &lbl_s);
                            lbl_f.set_label(
                                p.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                            );
                            *path.borrow_mut() = Some(p);
                        }
                    }
                });
            }
        });
    }

    // ── Cursor → status bar (Line N, Col N) ──────────────────────────────────
    {
        let lbl_s = lbl_status.clone();
        buffer.connect_mark_set(move |buf, _iter, mark| {
            if mark.name().as_deref() == Some("insert") {
                let cursor = buf.iter_at_mark(&buf.get_insert());
                lbl_s.set_label(&format!(
                    "Line {}, Col {}",
                    cursor.line() + 1,
                    cursor.line_offset() + 1
                ));
            }
        });
    }
}

/// Write buffer contents to `path`, updating the status label.
fn save_to_path(buf: &gtk4::TextBuffer, path: &PathBuf, lbl_s: &Label) {
    let (start, end) = buf.bounds();
    let text = buf.text(&start, &end, false);
    match std::fs::write(path, text.as_bytes()) {
        Ok(_)  => lbl_s.set_label(&format!("Saved: {}", path.display())),
        Err(e) => lbl_s.set_label(&format!("Save error: {e}")),
    }
}