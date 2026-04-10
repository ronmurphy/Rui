//! command_palette.rs — Searchable command palette (Ctrl+P).
//!
//! A VS Code–style popup listing all available actions with their keyboard
//! shortcuts.  Typing filters the list; pressing Enter or clicking a row
//! activates the action and dismisses the palette.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, EventControllerKey, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SearchEntry, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

/// A single entry in the command palette.
struct CommandEntry {
    /// Human-readable label shown in the list.
    label: &'static str,
    /// The GAction name (e.g. "app.save").
    action: &'static str,
    /// Shortcut hint shown on the right side.
    shortcut: &'static str,
}

/// All commands available in the palette.
const COMMANDS: &[CommandEntry] = &[
    // ── File ─────────────────────────────────────────────────────────
    CommandEntry { label: "New Tab",               action: "app.new-tab",          shortcut: "Ctrl+N"          },
    CommandEntry { label: "Open File",             action: "app.open",             shortcut: "Ctrl+O"          },
    CommandEntry { label: "Save",                  action: "app.save",             shortcut: "Ctrl+S"          },
    CommandEntry { label: "Save As",               action: "app.save-as",          shortcut: "Ctrl+Shift+S"    },
    CommandEntry { label: "Save All",              action: "app.save-all",         shortcut: ""                },
    CommandEntry { label: "Close Tab",             action: "app.close-tab",        shortcut: "Ctrl+W"          },
    // ── Project ──────────────────────────────────────────────────────
    CommandEntry { label: "New Project",           action: "app.new-project",      shortcut: ""                },
    CommandEntry { label: "Open Project",          action: "app.open-project",     shortcut: ""                },
    // ── Edit ─────────────────────────────────────────────────────────
    CommandEntry { label: "Find",                  action: "app.find",             shortcut: "Ctrl+F"          },
    CommandEntry { label: "Find and Replace",      action: "app.find-replace",     shortcut: "Ctrl+H"          },
    CommandEntry { label: "Search in Files",       action: "app.search-in-files",  shortcut: "Ctrl+Shift+F"    },
    CommandEntry { label: "Go to Line",            action: "app.goto-line",        shortcut: "Ctrl+G"          },
    CommandEntry { label: "Go to Definition",      action: "app.goto-definition",  shortcut: "F12"             },
    // ── View ─────────────────────────────────────────────────────────
    CommandEntry { label: "Toggle Sidebar",        action: "app.toggle-sidebar",   shortcut: "Ctrl+B"          },
    CommandEntry { label: "Toggle Output Panel",   action: "app.toggle-output",    shortcut: "Ctrl+J"          },
    CommandEntry { label: "Toggle Canvas",         action: "app.toggle-canvas",    shortcut: "Ctrl+Shift+U"    },
    CommandEntry { label: "Toggle Toolbox",        action: "app.toggle-toolbox",   shortcut: "Ctrl+Shift+T"    },
    CommandEntry { label: "Toggle Preview",        action: "app.toggle-preview",   shortcut: "Ctrl+Shift+P"    },
    CommandEntry { label: "Toggle Minimap",        action: "app.toggle-minimap",   shortcut: "Ctrl+M"          },
    CommandEntry { label: "Toggle Dark Mode",      action: "app.toggle-dark",      shortcut: "Ctrl+Shift+D"    },
    // ── Layout modes ─────────────────────────────────────────────────
    CommandEntry { label: "Code View",             action: "app.layout-code",      shortcut: "Ctrl+1"          },
    CommandEntry { label: "Designer View",         action: "app.layout-designer",  shortcut: "Ctrl+2"          },
    // ── Designer ─────────────────────────────────────────────────────
    CommandEntry { label: "Designer Undo",         action: "app.designer-undo",    shortcut: ""                },
    CommandEntry { label: "Designer Redo",         action: "app.designer-redo",    shortcut: ""                },
    CommandEntry { label: "Generate Handlers",     action: "app.generate-handlers",shortcut: ""                },
    CommandEntry { label: "Template Library",      action: "app.template-library", shortcut: ""                },
    CommandEntry { label: "CSS Bank",              action: "app.css-bank",         shortcut: ""                },
    // ── Build / Run ──────────────────────────────────────────────────
    CommandEntry { label: "Run",                   action: "app.run",              shortcut: "F5"              },
    CommandEntry { label: "Stop",                  action: "app.stop",             shortcut: "Shift+F5"        },
    CommandEntry { label: "Build",                 action: "app.build",            shortcut: "Ctrl+Shift+B"    },
    CommandEntry { label: "Build Install",         action: "app.build-install",    shortcut: ""                },
    CommandEntry { label: "Check",                 action: "app.check",            shortcut: ""                },
    CommandEntry { label: "Clippy",                action: "app.clippy",           shortcut: ""                },
    CommandEntry { label: "Format File",           action: "app.format-file",      shortcut: ""                },
    // ── AI ───────────────────────────────────────────────────────────
    CommandEntry { label: "Open AI Panel",         action: "app.ai-open",          shortcut: "Ctrl+Alt+A"      },
    CommandEntry { label: "Copy File to AI",       action: "app.ai-copy-file",     shortcut: "Ctrl+Alt+C"      },
    CommandEntry { label: "Apply AI Diff",         action: "app.ai-apply-diff",    shortcut: "Ctrl+Alt+D"      },
    // ── Help ─────────────────────────────────────────────────────────
    CommandEntry { label: "Help / Shortcuts",      action: "app.help",             shortcut: "F1"              },
];

/// Show the command palette as a transient popup attached to `parent`.
pub fn show(parent: &gtk4::ApplicationWindow) {
    let dialog = Window::builder()
        .transient_for(parent)
        .modal(true)
        .decorated(false)
        .default_width(480)
        .default_height(360)
        .build();

    let vbox = GtkBox::new(Orientation::Vertical, 0);
    vbox.add_css_class("command-palette");

    let search = SearchEntry::new();
    search.set_placeholder_text(Some("Type a command\u{2026}"));
    search.set_margin_start(8);
    search.set_margin_end(8);
    search.set_margin_top(8);
    search.set_margin_bottom(4);
    vbox.append(&search);

    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .min_content_height(280)
        .build();

    let listbox = ListBox::new();
    listbox.add_css_class("rich-list");

    // Build rows
    for entry in COMMANDS {
        let row_box = GtkBox::new(Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let name_label = Label::new(Some(entry.label));
        name_label.set_halign(gtk4::Align::Start);
        name_label.set_hexpand(true);

        let shortcut_label = Label::new(Some(entry.shortcut));
        shortcut_label.set_halign(gtk4::Align::End);
        shortcut_label.add_css_class("dim-label");
        shortcut_label.add_css_class("monospace");

        row_box.append(&name_label);
        row_box.append(&shortcut_label);

        let row = ListBoxRow::new();
        row.set_child(Some(&row_box));
        listbox.append(&row);
    }

    // Filter
    let filter_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    listbox.set_filter_func({
        let q = filter_query.clone();
        move |row| {
            let q = q.borrow();
            if q.is_empty() {
                return true;
            }
            // Get the label text from the first child of the row's box
            row.child()
                .and_then(|c| c.downcast::<GtkBox>().ok())
                .and_then(|b| b.first_child())
                .and_then(|c| c.downcast::<Label>().ok())
                .map(|l| l.text().to_ascii_lowercase().contains(q.as_str()))
                .unwrap_or(false)
        }
    });

    {
        let lb = listbox.clone();
        let fq = filter_query.clone();
        search.connect_search_changed(move |s| {
            *fq.borrow_mut() = s.text().to_ascii_lowercase().to_string();
            lb.invalidate_filter();
            // Auto-select the first visible row
            if let Some(first) = lb.row_at_index(0) {
                lb.select_row(Some(&first));
            }
        });
    }

    // Activate on row click
    {
        let dlg = dialog.clone();
        let parent_ref = parent.clone();
        listbox.connect_row_activated(move |_, row| {
            let idx = row.index() as usize;
            if let Some(cmd) = find_command_at_visible_index(&row.parent().unwrap().downcast::<ListBox>().unwrap(), idx) {
                dlg.close();
                activate_action(&parent_ref, cmd.action);
            }
        });
    }

    // Keyboard handling: Enter to activate, Escape to close, arrows work naturally
    {
        let dlg = dialog.clone();
        let lb = listbox.clone();
        let parent_ref = parent.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            match key {
                gtk4::gdk::Key::Escape => {
                    dlg.close();
                    gtk4::glib::Propagation::Stop
                }
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                    if let Some(row) = lb.selected_row() {
                        let idx = row.index();
                        // Walk visible rows to find the command
                        if let Some(cmd) = visible_command_at(&lb, idx) {
                            dlg.close();
                            activate_action(&parent_ref, cmd.action);
                        }
                    }
                    gtk4::glib::Propagation::Stop
                }
                gtk4::gdk::Key::Down | gtk4::gdk::Key::Up => {
                    // Let the listbox handle arrow keys — move focus there
                    lb.grab_focus();
                    gtk4::glib::Propagation::Proceed
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });
        search.add_controller(key_ctrl);
    }

    scroll.set_child(Some(&listbox));
    vbox.append(&scroll);
    dialog.set_child(Some(&vbox));

    // Select first row initially
    if let Some(first) = listbox.row_at_index(0) {
        listbox.select_row(Some(&first));
    }

    dialog.present();
    search.grab_focus();
}

/// Find the CommandEntry for a given row in the listbox by its visible position.
fn find_command_at_visible_index(lb: &ListBox, row_idx: usize) -> Option<&'static CommandEntry> {
    // The row_idx from activated is the absolute index, but some rows may be filtered.
    // Map it back to COMMANDS by counting visible rows.
    let mut visible_count = 0;
    for (i, cmd) in COMMANDS.iter().enumerate() {
        if let Some(row) = lb.row_at_index(i as i32) {
            if row.is_visible() {
                if i == row_idx {
                    return Some(cmd);
                }
                visible_count += 1;
            }
        }
    }
    let _ = visible_count;
    None
}

/// Find command at a given absolute row index.
fn visible_command_at(_lb: &ListBox, row_idx: i32) -> Option<&'static CommandEntry> {
    COMMANDS.get(row_idx as usize)
}

/// Activate a GAction by name (e.g. "app.save").
fn activate_action(window: &gtk4::ApplicationWindow, action_name: &str) {
    if let Some(stripped) = action_name.strip_prefix("app.") {
        if let Some(app) = window.application() {
            app.activate_action(stripped, None);
        }
    }
}
