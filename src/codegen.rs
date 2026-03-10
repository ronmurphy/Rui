//! codegen.rs — Generate idiomatic Rust handler stubs for .ui widgets.
//!
//! Given a `.ui` XML string, walks the tree with `roxmltree`, finds
//! interactive widgets (Button, ToggleButton, Switch, Entry, etc.),
//! and emits Rust code that connects their signals.
//!
//! Two public entry points:
//! - `generate_all_handlers(xml)` → full companion `-app.rs` source
//! - `handler_for_widget(class, id)` → single handler snippet + signal connect

use std::collections::BTreeMap;
use std::path::Path;

/// Description of one widget that needs a handler.
#[derive(Debug, Clone)]
pub struct HandlerInfo {
    /// GTK class, e.g. "GtkButton"
    pub class: String,
    /// The id="…" attribute (may be empty for anonymous widgets)
    pub id: String,
    /// Rust function name for the handler, e.g. `on_save_button_clicked`
    pub fn_name: String,
    /// The GTK signal to connect, e.g. "clicked"
    pub signal: String,
    /// The `connect_*` method name, e.g. "connect_clicked"
    pub connect_method: String,
    /// Extra type annotation for the closure parameter
    pub closure_params: String,
}

/// Map a GTK class name to its primary interactive signal.
fn signal_for_class(class: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // Returns (signal_name, connect_method, closure_params)
    match class {
        "GtkButton"       => Some(("clicked",  "connect_clicked",  "|button|")),
        "GtkToggleButton" => Some(("toggled",  "connect_toggled",  "|button|")),
        "GtkCheckButton"  => Some(("toggled",  "connect_toggled",  "|button|")),
        "GtkSwitch"       => Some(("state-set","connect_state_set","|switch, state|")),
        "GtkEntry"        => Some(("activate", "connect_activate", "|entry|")),
        "GtkSearchEntry"  => Some(("activate", "connect_activate", "|entry|")),
        "GtkScale"        => Some(("value-changed","connect_value_changed","|range|")),
        "GtkSpinButton"   => Some(("value-changed","connect_value_changed","|spin|")),
        "GtkDropDown"     => Some(("notify::selected","connect_selected_notify","|dropdown|")),
        "GtkComboBoxText" => Some(("changed",  "connect_changed",  "|combo|")),
        "GtkLinkButton"   => Some(("activate-link","connect_activate_link","|button|")),
        "GtkMenuButton"   => None, // typically wired via popover, skip
        "GtkExpander"     => Some(("notify::expanded","connect_expanded_notify","|expander|")),
        "GtkNotebook"     => Some(("switch-page","connect_switch_page","|notebook, _page, page_num|")),
        "GtkTextView"     => None, // buffer signals are more common, skip simple codegen
        _ => None,
    }
}

/// Walk a .ui XML string and collect all interactive widgets.
pub fn collect_handlers(xml: &str) -> Vec<HandlerInfo> {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut handlers = Vec::new();
    let mut id_counts: BTreeMap<String, usize> = BTreeMap::new();

    for node in doc.descendants() {
        if !node.is_element() || node.tag_name().name() != "object" {
            continue;
        }
        let class = match node.attribute("class") {
            Some(c) => c,
            None => continue,
        };
        let (signal, connect_method, closure_params) = match signal_for_class(class) {
            Some(t) => t,
            None => continue,
        };

        let id = node.attribute("id").unwrap_or("").to_string();

        // Build a readable function name
        let fn_name = make_fn_name(class, &id, &mut id_counts);

        handlers.push(HandlerInfo {
            class: class.to_string(),
            id,
            fn_name,
            signal: signal.to_string(),
            connect_method: connect_method.to_string(),
            closure_params: closure_params.to_string(),
        });
    }

    handlers
}

/// Build a handler function name from class + id.
/// e.g. id="save_button" → "on_save_button_clicked"
///      id=""             → "on_button_clicked" (with dedup counter)
fn make_fn_name(class: &str, id: &str, counts: &mut BTreeMap<String, usize>) -> String {
    make_fn_name_inner(class, id, counts)
}

/// Public version so app.rs can compute the same function name
/// when jumping to a handler after double-click.
pub fn make_fn_name_pub(class: &str, id: &str, counts: &mut BTreeMap<String, usize>) -> String {
    make_fn_name_inner(class, id, counts)
}

/// Returns true if this widget class has a known signal mapping for codegen.
pub fn has_signal(class: &str) -> bool {
    signal_for_class(class).is_some()
}

fn make_fn_name_inner(class: &str, id: &str, counts: &mut BTreeMap<String, usize>) -> String {
    let base = if !id.is_empty() {
        // Sanitise the id to a valid Rust identifier
        let clean: String = id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        clean
    } else {
        // Derive from class: "GtkToggleButton" → "toggle_button"
        let short = class.strip_prefix("Gtk").unwrap_or(class);
        camel_to_snake(short)
    };

    let verb = signal_for_class(class)
        .map(|(signal_word, _, _)| {
            signal_word.split('-').next().unwrap_or("activated")
        })
        .unwrap_or("activated");

    let candidate = format!("on_{}_{}", base, verb);

    // De-duplicate anonymous widgets
    let count = counts.entry(candidate.clone()).or_insert(0);
    *count += 1;
    if *count > 1 {
        format!("{}_{}", candidate, count)
    } else {
        candidate
    }
}

/// Convert CamelCase to snake_case.
fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Generate a full companion Rust file with all handler stubs.
///
/// The generated file imports gtk4 prelude and contains:
/// - One `pub fn connect_handlers(…)` that wires all signals
/// - One `fn on_widget_signal(…)` stub per interactive widget
pub fn generate_all_handlers(xml: &str, ui_filename: &str) -> String {
    let handlers = collect_handlers(xml);

    // Always emit connect_handlers so `main.rs` can call it unconditionally.
    if handlers.is_empty() {
        return format!(
            "//! Handler stubs for {ui_filename}\n\
             //!\n\
             //! Auto-generated by Rui. Fill in handler bodies below.\n\n\
             /// Wire all widget signals to their handler functions.\n\
             ///\n\
             /// Call this after building your UI from the .ui file.\n\
             #[allow(unused_imports)]\n\
             use gtk4::prelude::*;\n\n\
             pub fn connect_handlers(_builder: &gtk4::Builder) {{\n\
             \x20   // No interactive widgets with signals found in the .ui file.\n\
             }}\n"
        );
    }

    let mut out = String::new();

    // Header
    out.push_str(&format!("//! Handler stubs for {ui_filename}\n"));
    out.push_str("//!\n");
    out.push_str("//! Auto-generated by Rui. Fill in the handler bodies below.\n");
    out.push_str("//! Re-running \"Generate All Handlers\" will NOT overwrite\n");
    out.push_str("//! existing handler functions — only missing ones are appended.\n\n");

    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use gtk4::prelude::*;\n");

    // Collect unique GTK types we'll need to import
    let mut types: Vec<&str> = Vec::new();
    for h in &handlers {
        let short = h.class.strip_prefix("Gtk").unwrap_or(&h.class);
        if !types.contains(&short) {
            types.push(short);
        }
    }
    types.sort();
    out.push_str(&format!("use gtk4::{{{}}};\n\n", types.join(", ")));

    // ── connect_handlers() ──
    out.push_str("/// Wire all widget signals to their handler functions.\n");
    out.push_str("///\n");
    out.push_str("/// Call this after building your UI from the .ui file.\n");
    out.push_str("/// You will need to obtain references to each widget\n");
    out.push_str("/// (e.g. via `builder.object::<Button>(\"id\")`).\n");
    out.push_str("pub fn connect_handlers(builder: &gtk4::Builder) {\n");

    for h in &handlers {
        let short = h.class.strip_prefix("Gtk").unwrap_or(&h.class);
        if !h.id.is_empty() {
            out.push_str(&format!(
                "    if let Some(w) = builder.object::<{short}>(\"{id}\") {{\n\
                 \x20       w.{connect}(move {params} {{\n\
                 \x20           {fn_name}(&w);\n\
                 \x20       }});\n\
                 \x20   }}\n\n",
                short = short,
                id = h.id,
                connect = h.connect_method,
                params = h.closure_params,
                fn_name = h.fn_name,
            ));
        } else {
            out.push_str(&format!(
                "    // TODO: obtain a reference to the anonymous {short} and connect:\n\
                 \x20   // widget.{connect}(move {params} {{\n\
                 \x20   //     {fn_name}(…);\n\
                 \x20   // }});\n\n",
                short = short,
                connect = h.connect_method,
                params = h.closure_params,
                fn_name = h.fn_name,
            ));
        }
    }

    out.push_str("}\n\n");

    // ── Individual handler stubs ──
    for h in &handlers {
        let short = h.class.strip_prefix("Gtk").unwrap_or(&h.class);
        let id_comment = if h.id.is_empty() {
            String::new()
        } else {
            format!(" (id=\"{}\")", h.id)
        };

        // Special return type for state-set (Switch)
        let ret = if h.signal == "state-set" || h.signal == "activate-link" {
            " -> gtk4::glib::Propagation"
        } else {
            ""
        };
        let ret_body = if h.signal == "state-set" || h.signal == "activate-link" {
            "\n    gtk4::glib::Propagation::Proceed"
        } else {
            ""
        };

        out.push_str(&format!(
            "/// Handler for {class}{id_comment} — signal \"{signal}\"\n\
             fn {fn_name}(widget: &{short}){ret} {{\n\
             \x20   // TODO: implement handler{ret_body}\n\
             }}\n\n",
            class = h.class,
            id_comment = id_comment,
            signal = h.signal,
            fn_name = h.fn_name,
            short = short,
            ret = ret,
            ret_body = ret_body,
        ));
    }

    out
}

/// Generate the handler snippet for a single widget (used by double-click).
pub fn handler_for_widget(class: &str, id: &str) -> Option<String> {
    let (signal, _connect_method, _closure_params) = signal_for_class(class)?;
    let mut counts = BTreeMap::new();
    let fn_name = make_fn_name(class, id, &mut counts);
    let short = class.strip_prefix("Gtk").unwrap_or(class);

    let ret = if signal == "state-set" || signal == "activate-link" {
        " -> gtk4::glib::Propagation"
    } else {
        ""
    };
    let ret_body = if signal == "state-set" || signal == "activate-link" {
        "\n    gtk4::glib::Propagation::Proceed"
    } else {
        ""
    };

    let id_comment = if id.is_empty() {
        String::new()
    } else {
        format!(" (id=\"{}\")", id)
    };

    let snippet = format!(
        "/// Handler for {class}{id_comment} — signal \"{signal}\"\n\
         fn {fn_name}(widget: &{short}){ret} {{\n\
         \x20   // TODO: implement handler{ret_body}\n\
         }}\n",
        class = class,
        id_comment = id_comment,
        signal = signal,
        fn_name = fn_name,
        short = short,
        ret = ret,
        ret_body = ret_body,
    );
    Some(snippet)
}

/// Compute the companion handler file path for a given `.ui` file.
///
/// Project structure (has `src/` dir): `my_app/layout.ui` → `my_app/src/layout_app.rs`
/// Legacy / standalone:                `dir/form.ui`      → `dir/form-app.rs`
pub fn companion_path(ui_path: &Path) -> std::path::PathBuf {
    let stem = ui_path.file_stem().unwrap_or_default().to_string_lossy();
    let parent = ui_path.parent().unwrap_or(Path::new(""));

    // Legacy: if a <stem>-app.rs already exists next to the .ui, keep using it
    let legacy = parent.join(format!("{}-app.rs", stem));
    if legacy.exists() {
        return legacy;
    }

    // Project structure: if src/ exists, put module there with underscores
    let src_dir = parent.join("src");
    if src_dir.is_dir() {
        let module_name = stem.replace('-', "_");
        return src_dir.join(format!("{}_app.rs", module_name));
    }

    // Fallback: companion next to the .ui file
    legacy
}

/// Generate a starter GtkBox .ui template.
pub fn make_box_template() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="main_box">
    <property name="orientation">vertical</property>
    <property name="spacing">8</property>
    <property name="margin-start">12</property>
    <property name="margin-end">12</property>
    <property name="margin-top">12</property>
    <property name="margin-bottom">12</property>
  </object>
</interface>
"#
    .to_string()
}

/// Generate a starter GtkGrid .ui template with the given dimensions.
/// Stores `rui-rows` / `rui-columns` as custom properties so the canvas
/// knows the intended size before any widgets are placed.
pub fn make_grid_template(rows: i32, cols: i32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkGrid" id="main_grid">
    <property name="row-spacing">8</property>
    <property name="column-spacing">8</property>
    <property name="margin-start">12</property>
    <property name="margin-end">12</property>
    <property name="margin-top">12</property>
    <property name="margin-bottom">12</property>
    <property name="rui-rows">{rows}</property>
    <property name="rui-columns">{cols}</property>
  </object>
</interface>
"#
    )
}

/// Scaffold a new GTK4 Rust project in the given directory.
///
/// Creates: `Cargo.toml`, `layout.ui`, `src/main.rs`, `src/layout_app.rs`
/// Pass a `ui` string (from `make_box_template` or `make_grid_template`) to
/// control what `layout.ui` contains.
/// Find the id of the first `<object>` element in a .ui XML string.
/// Falls back to "main_box" if none is found.
fn root_widget_id(ui: &str) -> String {
    if let Ok(doc) = roxmltree::Document::parse(ui) {
        for node in doc.descendants() {
            if node.is_element() && node.tag_name().name() == "object" {
                if let Some(id) = node.attribute("id") {
                    return id.to_string();
                }
            }
        }
    }
    "main_box".to_string()
}

pub fn scaffold_project(dir: &Path, project_name: &str, ui: &str) -> std::io::Result<()> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    // ── Cargo.toml ──
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
gtk4 = {{ version = "0.9", features = ["v4_10"] }}
"#,
        name = project_name
    );
    std::fs::write(dir.join("Cargo.toml"), &cargo_toml)?;

    // ── layout.ui — caller-supplied template ──
    std::fs::write(dir.join("layout.ui"), ui)?;

    // ── src/layout_app.rs — handler stubs generated from the template ──
    let handlers = generate_all_handlers(ui, "layout.ui");
    std::fs::write(src_dir.join("layout_app.rs"), &handlers)?;

    // ── src/main.rs — GTK4 bootstrap ──
    let root_id = root_widget_id(ui);
    let main_rs = format!(
        r#"use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {{
    let app = Application::builder()
        .application_id("com.example.{name}")
        .build();

    app.connect_activate(|app| {{
        let ui = include_str!("../layout.ui");
        // Strip Rui design-time metadata (rui-*) before passing to GTK Builder.
        let ui_clean: String = ui
            .lines()
            .filter(|l| !l.trim_start().starts_with("<property name=\"rui-"))
            .collect::<Vec<_>>()
            .join("\n");
        let builder = gtk4::Builder::from_string(&ui_clean);

        layout_app::connect_handlers(&builder);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("{title}")
            .default_width(400)
            .default_height(300)
            .build();

        if let Some(root) = builder.object::<gtk4::Widget>("{root_id}") {{
            window.set_child(Some(&root));
        }}

        window.present();
    }});

    app.run();
}}
"#,
        name = project_name,
        title = project_name,
        root_id = root_id,
    );
    std::fs::write(src_dir.join("main.rs"), &main_rs)?;

    Ok(())
}

/// Merge new handler stubs into an existing companion file.
/// Only appends functions that are not already present (by fn name).
pub fn merge_handlers(existing: &str, xml: &str, ui_filename: &str) -> String {
    let handlers = collect_handlers(xml);
    if handlers.is_empty() {
        return existing.to_string();
    }

    // Find which handler fn names already exist in the file
    let mut missing: Vec<&HandlerInfo> = Vec::new();
    for h in &handlers {
        let pattern = format!("fn {}(", h.fn_name);
        if !existing.contains(&pattern) {
            missing.push(h);
        }
    }

    if missing.is_empty() {
        return existing.to_string();
    }

    let mut out = existing.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "\n// ── New handlers (generated by Rui for {}) ─────\n\n",
        ui_filename
    ));

    for h in &missing {
        let short = h.class.strip_prefix("Gtk").unwrap_or(&h.class);
        let id_comment = if h.id.is_empty() {
            String::new()
        } else {
            format!(" (id=\"{}\")", h.id)
        };
        let ret = if h.signal == "state-set" || h.signal == "activate-link" {
            " -> gtk4::glib::Propagation"
        } else {
            ""
        };
        let ret_body = if h.signal == "state-set" || h.signal == "activate-link" {
            "\n    gtk4::glib::Propagation::Proceed"
        } else {
            ""
        };
        out.push_str(&format!(
            "/// Handler for {class}{id_comment} — signal \"{signal}\"\n\
             fn {fn_name}(widget: &{short}){ret} {{\n\
             \x20   // TODO: implement handler{ret_body}\n\
             }}\n\n",
            class = h.class,
            id_comment = id_comment,
            signal = h.signal,
            fn_name = h.fn_name,
            short = short,
            ret = ret,
            ret_body = ret_body,
        ));
    }

    out
}

/// Find the byte offset of a handler function in a companion file source.
/// Returns the byte offset of the `fn ` keyword, or None.
pub fn find_handler_offset(source: &str, fn_name: &str) -> Option<usize> {
    let pattern = format!("fn {}(", fn_name);
    source.find(&pattern)
}

// ── Template Library ───────────────────────────────────────────────

/// Pre-built UI snippet templates.
struct Template {
    name: &'static str,
    description: &'static str,
    xml: &'static str,
}

const TEMPLATES: &[Template] = &[
    Template {
        name: "Login Form",
        description: "Username + password entries with a Login button",
        xml: r#"    <child>
      <object class="GtkBox" id="login_form">
        <property name="orientation">vertical</property>
        <property name="spacing">8</property>
        <property name="margin-start">24</property>
        <property name="margin-end">24</property>
        <property name="margin-top">24</property>
        <property name="margin-bottom">24</property>
        <child>
          <object class="GtkLabel" id="login_title">
            <property name="label">Sign In</property>
            <attributes>
              <attribute name="weight" value="bold"/>
              <attribute name="scale" value="1.4"/>
            </attributes>
          </object>
        </child>
        <child>
          <object class="GtkEntry" id="username_entry">
            <property name="placeholder-text">Username</property>
          </object>
        </child>
        <child>
          <object class="GtkPasswordEntry" id="password_entry">
            <property name="placeholder-text">Password</property>
            <property name="show-peek-icon">true</property>
          </object>
        </child>
        <child>
          <object class="GtkButton" id="login_button">
            <property name="label">Log In</property>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Header Bar",
        description: "Titlebar with a title and close button",
        xml: r#"    <child>
      <object class="GtkHeaderBar" id="header_bar">
        <property name="show-title-buttons">true</property>
        <child type="title">
          <object class="GtkLabel" id="header_title">
            <property name="label">My Application</property>
            <attributes>
              <attribute name="weight" value="bold"/>
            </attributes>
          </object>
        </child>
        <child type="start">
          <object class="GtkButton" id="back_button">
            <property name="icon-name">go-previous-symbolic</property>
          </object>
        </child>
        <child type="end">
          <object class="GtkButton" id="menu_button">
            <property name="icon-name">open-menu-symbolic</property>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "List with Scrollbar",
        description: "Scrollable list view with a model placeholder",
        xml: r#"    <child>
      <object class="GtkScrolledWindow" id="list_scroll">
        <property name="hexpand">true</property>
        <property name="vexpand">true</property>
        <property name="min-content-height">200</property>
        <child>
          <object class="GtkListBox" id="item_list">
            <property name="selection-mode">single</property>
            <child>
              <object class="GtkLabel">
                <property name="label">Item 1</property>
                <property name="xalign">0</property>
              </object>
            </child>
            <child>
              <object class="GtkLabel">
                <property name="label">Item 2</property>
                <property name="xalign">0</property>
              </object>
            </child>
            <child>
              <object class="GtkLabel">
                <property name="label">Item 3</property>
                <property name="xalign">0</property>
              </object>
            </child>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Search Bar",
        description: "Search entry with a filter toggle",
        xml: r#"    <child>
      <object class="GtkBox" id="search_box">
        <property name="orientation">horizontal</property>
        <property name="spacing">4</property>
        <child>
          <object class="GtkSearchEntry" id="search_entry">
            <property name="hexpand">true</property>
            <property name="placeholder-text">Search…</property>
          </object>
        </child>
        <child>
          <object class="GtkToggleButton" id="filter_toggle">
            <property name="icon-name">funnel-symbolic</property>
            <property name="tooltip-text">Toggle filters</property>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Form Grid",
        description: "Two-column label + entry form layout",
        xml: r#"    <child>
      <object class="GtkGrid" id="form_grid">
        <property name="row-spacing">6</property>
        <property name="column-spacing">12</property>
        <property name="margin-start">12</property>
        <property name="margin-end">12</property>
        <property name="margin-top">12</property>
        <property name="margin-bottom">12</property>
        <child>
          <object class="GtkLabel">
            <property name="label">Name:</property>
            <property name="xalign">1</property>
            <layout>
              <property name="column">0</property>
              <property name="row">0</property>
            </layout>
          </object>
        </child>
        <child>
          <object class="GtkEntry" id="name_entry">
            <property name="hexpand">true</property>
            <layout>
              <property name="column">1</property>
              <property name="row">0</property>
            </layout>
          </object>
        </child>
        <child>
          <object class="GtkLabel">
            <property name="label">Email:</property>
            <property name="xalign">1</property>
            <layout>
              <property name="column">0</property>
              <property name="row">1</property>
            </layout>
          </object>
        </child>
        <child>
          <object class="GtkEntry" id="email_entry">
            <property name="hexpand">true</property>
            <layout>
              <property name="column">1</property>
              <property name="row">1</property>
            </layout>
          </object>
        </child>
        <child>
          <object class="GtkButton" id="submit_button">
            <property name="label">Submit</property>
            <property name="halign">end</property>
            <layout>
              <property name="column">1</property>
              <property name="row">2</property>
            </layout>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Image + Caption",
        description: "Picture widget with a caption label below",
        xml: r#"    <child>
      <object class="GtkBox" id="image_box">
        <property name="orientation">vertical</property>
        <property name="spacing">4</property>
        <property name="halign">center</property>
        <child>
          <object class="GtkPicture" id="main_picture">
            <property name="content-fit">contain</property>
            <property name="width-request">320</property>
            <property name="height-request">240</property>
          </object>
        </child>
        <child>
          <object class="GtkLabel" id="caption_label">
            <property name="label">Caption goes here</property>
            <property name="wrap">true</property>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Button Row",
        description: "Horizontal row of OK / Cancel / Help buttons",
        xml: r#"    <child>
      <object class="GtkBox" id="button_row">
        <property name="orientation">horizontal</property>
        <property name="spacing">8</property>
        <property name="halign">end</property>
        <property name="margin-top">12</property>
        <child>
          <object class="GtkButton" id="help_button">
            <property name="label">Help</property>
          </object>
        </child>
        <child>
          <object class="GtkButton" id="cancel_button">
            <property name="label">Cancel</property>
          </object>
        </child>
        <child>
          <object class="GtkButton" id="ok_button">
            <property name="label">OK</property>
          </object>
        </child>
      </object>
    </child>"#,
    },
    Template {
        name: "Notebook (Tabbed)",
        description: "Tabbed container with two placeholder pages",
        xml: r#"    <child>
      <object class="GtkNotebook" id="tab_notebook">
        <child>
          <object class="GtkNotebookPage">
            <property name="child">
              <object class="GtkLabel" id="page1_content">
                <property name="label">Page 1 content</property>
              </object>
            </property>
            <property name="tab">
              <object class="GtkLabel">
                <property name="label">Tab 1</property>
              </object>
            </property>
          </object>
        </child>
        <child>
          <object class="GtkNotebookPage">
            <property name="child">
              <object class="GtkLabel" id="page2_content">
                <property name="label">Page 2 content</property>
              </object>
            </property>
            <property name="tab">
              <object class="GtkLabel">
                <property name="label">Tab 2</property>
              </object>
            </property>
          </object>
        </child>
      </object>
    </child>"#,
    },
];

/// Show a modal Template Library dialog.
/// User picks a template; its XML is inserted at the cursor in the current buffer.
pub fn show_template_library(
    parent: &gtk4::ApplicationWindow,
    notebook: &crate::notebook::NotebookManager,
    cfg: &crate::config::EditorConfig,
) {
    use gtk4::prelude::*;
    use gtk4::{
        Dialog, Label, ListBox, Orientation, Box as GtkBox,
        ScrolledWindow, ResponseType,
    };

    let dialog = Dialog::builder()
        .title("Template Library")
        .transient_for(parent)
        .modal(true)
        .default_width(460)
        .default_height(420)
        .use_header_bar(1)
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    let insert_btn = dialog.add_button("Insert", ResponseType::Accept);
    insert_btn.add_css_class("suggested-action");

    let content = dialog.content_area();
    content.set_spacing(4);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_margin_top(8);
    content.set_margin_bottom(8);

    let desc_label = Label::new(Some("Choose a template to insert at the cursor position."));
    desc_label.set_halign(gtk4::Align::Start);
    desc_label.set_margin_bottom(4);
    content.append(&desc_label);

    let listbox = ListBox::new();
    listbox.set_selection_mode(gtk4::SelectionMode::Single);

    for tmpl in TEMPLATES {
        let row_box = GtkBox::new(Orientation::Vertical, 2);
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);

        let name_lbl = Label::new(Some(tmpl.name));
        name_lbl.set_halign(gtk4::Align::Start);
        name_lbl.add_css_class("heading");

        let desc_lbl = Label::new(Some(tmpl.description));
        desc_lbl.set_halign(gtk4::Align::Start);
        desc_lbl.add_css_class("dim-label");

        row_box.append(&name_lbl);
        row_box.append(&desc_lbl);
        listbox.append(&row_box);
    }

    listbox.select_row(listbox.row_at_index(0).as_ref());

    let scroll = ScrolledWindow::builder()
        .child(&listbox)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(300)
        .build();
    content.append(&scroll);
    content.show();

    let nb = notebook.clone();
    let _cfg = cfg.clone();
    let lb = listbox.clone();
    dialog.connect_response(move |dlg, resp| {
        if resp == ResponseType::Accept {
            if let Some(row) = lb.selected_row() {
                let idx = row.index() as usize;
                if idx < TEMPLATES.len() {
                    let xml = TEMPLATES[idx].xml;
                    // Insert into current tab's buffer at cursor
                    if let Some(tab) = nb.current_tab() {
                        let buf = tab.buffer();
                        let mut cursor = buf.iter_at_mark(&buf.get_insert());
                        buf.begin_user_action();
                        buf.insert(&mut cursor, "\n");
                        buf.insert(&mut cursor, xml);
                        buf.insert(&mut cursor, "\n");
                        buf.end_user_action();
                    }
                }
            }
        }
        dlg.close();
    });

    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("ToggleButton"), "toggle_button");
        assert_eq!(camel_to_snake("SpinButton"), "spin_button");
        assert_eq!(camel_to_snake("Button"), "button");
    }

    #[test]
    fn test_collect_handlers_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="main_box">
    <child>
      <object class="GtkButton" id="save_button">
        <property name="label">Save</property>
      </object>
    </child>
    <child>
      <object class="GtkSwitch" id="dark_mode"/>
    </child>
    <child>
      <object class="GtkLabel" id="info_label">
        <property name="label">Hello</property>
      </object>
    </child>
  </object>
</interface>"#;
        let handlers = collect_handlers(xml);
        assert_eq!(handlers.len(), 2); // Button + Switch, not Label
        assert_eq!(handlers[0].fn_name, "on_save_button_clicked");
        assert_eq!(handlers[1].fn_name, "on_dark_mode_state");
    }

    #[test]
    fn test_companion_path() {
        // Non-existent dirs — falls back to legacy <stem>-app.rs
        let p = companion_path(Path::new("/tmp/_rui_test_fake/my-app.ui"));
        assert_eq!(p, std::path::PathBuf::from("/tmp/_rui_test_fake/my-app-app.rs"));

        let p = companion_path(Path::new("/tmp/_rui_test_fake/form.ui"));
        assert_eq!(p, std::path::PathBuf::from("/tmp/_rui_test_fake/form-app.rs"));
    }
}
