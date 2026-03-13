//! Pre-defined layout templates for new .ui files.
//!
//! Each template is a function of (rows, cols) that returns a ready-to-use
//! .ui XML string with merged regions and starter widgets already placed.

pub struct Template {
    pub id:          &'static str,
    pub label:       &'static str,
    pub description: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        id:          "blank",
        label:       "Blank",
        description: "Empty grid, no widgets placed",
    },
    Template {
        id:          "header-content",
        label:       "Header + Content",
        description: "Merged header bar across the top, open content area below",
    },
    Template {
        id:          "header-content-footer",
        label:       "Header + Content + Footer",
        description: "Merged header, open content rows, merged footer/status bar",
    },
    Template {
        id:          "text-editor",
        label:       "Text Editor",
        description: "Header bar, left sidebar column, scrollable text area",
    },
    Template {
        id:          "two-panel",
        label:       "Two-Panel",
        description: "Header bar, left panel and right panel side by side",
    },
    Template {
        id:          "dashboard",
        label:       "Dashboard",
        description: "Header bar, then a 2×2 tile grid of content panels",
    },
    Template {
        id:          "game-canvas",
        label:       "Game Canvas",
        description: "Score/title bar, large central play area, controls row at bottom",
    },
    Template {
        id:          "media-player",
        label:       "Media Player",
        description: "Header bar, large media view, seek bar and playback controls below",
    },
    Template {
        id:          "chat",
        label:       "Chat / Messaging",
        description: "Header bar, scrollable message list, input entry + send button",
    },
    Template {
        id:          "file-browser",
        label:       "File Browser",
        description: "Header bar, path toolbar, folder tree + file list, status bar",
    },
];

/// Return the .ui XML for `id` applied to a `rows × cols` grid.
/// Falls back to a blank grid for unknown ids or GtkBox layouts.
pub fn apply(id: &str, rows: i32, cols: i32) -> String {
    match id {
        "header-content"         => header_content(rows, cols),
        "header-content-footer"  => header_content_footer(rows, cols),
        "text-editor"            => text_editor(rows, cols),
        "two-panel"              => two_panel(rows, cols),
        "dashboard"              => dashboard(rows, cols),
        "game-canvas"            => game_canvas(rows, cols),
        "media-player"           => media_player(rows, cols),
        "chat"                   => chat(rows, cols),
        "file-browser"           => file_browser(rows, cols),
        _                        => crate::codegen::make_grid_template(rows, cols),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a <child> block for a widget placed in the grid.
fn child(class: &str, id: &str, col: i32, row: i32, cs: i32, rs: i32, extra_props: &str) -> String {
    format!(
        "    <child>\n\
               <object class=\"{class}\" id=\"{id}\">\n\
         {extra_props}\
                 <layout>\n\
                   <property name=\"column\">{col}</property>\n\
                   <property name=\"row\">{row}</property>\n\
                   <property name=\"column-span\">{cs}</property>\n\
                   <property name=\"row-span\">{rs}</property>\n\
                 </layout>\n\
               </object>\n\
             </child>\n"
    )
}

/// Generate a merged-cell placeholder (same format as apply_merge produces).
fn merged(col: i32, row: i32, cs: i32, rs: i32) -> String {
    child(
        "GtkLabel",
        &format!("merged_c{}_r{}", col, row),
        col, row, cs, rs,
        "        <property name=\"label\"></property>\n\
                 <property name=\"rui-merged\">true</property>\n",
    )
}

/// Wrap children inside a grid template.
fn grid_xml(rows: i32, cols: i32, children: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <interface>\n\
           <requires lib=\"gtk\" version=\"4.0\"/>\n\
           <object class=\"GtkGrid\" id=\"main_grid\">\n\
             <property name=\"row-spacing\">8</property>\n\
             <property name=\"column-spacing\">8</property>\n\
             <property name=\"margin-start\">12</property>\n\
             <property name=\"margin-end\">12</property>\n\
             <property name=\"margin-top\">12</property>\n\
             <property name=\"margin-bottom\">12</property>\n\
             <property name=\"hexpand\">true</property>\n\
             <property name=\"vexpand\">true</property>\n\
             <property name=\"rui-rows\">{rows}</property>\n\
             <property name=\"rui-columns\">{cols}</property>\n\
         {children}\
           </object>\n\
         </interface>\n"
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Templates
// ─────────────────────────────────────────────────────────────────────────────

/// Header row merged across all columns, content rows below.
fn header_content(rows: i32, cols: i32) -> String {
    let rows = rows.max(2);
    let mut c = String::new();
    // HeaderBar spanning full top row
    c.push_str(&child(
        "GtkHeaderBar", "header_bar",
        0, 0, cols, 1,
        "        <property name=\"hexpand\">true</property>\n",
    ));
    // Merged placeholder for content area
    if rows > 1 {
        c.push_str(&merged(0, 1, cols, rows - 1));
    }
    grid_xml(rows, cols, &c)
}

/// Header + open content + footer.
fn header_content_footer(rows: i32, cols: i32) -> String {
    let rows = rows.max(3);
    let content_rows = rows - 2;
    let mut c = String::new();
    c.push_str(&child(
        "GtkHeaderBar", "header_bar",
        0, 0, cols, 1,
        "        <property name=\"hexpand\">true</property>\n",
    ));
    if content_rows > 0 {
        c.push_str(&merged(0, 1, cols, content_rows));
    }
    c.push_str(&child(
        "GtkLabel", "status_label",
        0, rows - 1, cols, 1,
        "        <property name=\"label\">Status / Footer</property>\n\
                 <property name=\"xalign\">0</property>\n",
    ));
    grid_xml(rows, cols, &c)
}

/// Header bar + narrow sidebar column + text view.
fn text_editor(rows: i32, cols: i32) -> String {
    let rows = rows.max(2);
    let cols = cols.max(2);
    let sidebar_cols = (cols / 4).max(1);
    let content_cols = cols - sidebar_cols;
    let content_rows = rows - 1;

    let mut c = String::new();
    // Full-width header
    c.push_str(&child(
        "GtkHeaderBar", "header_bar",
        0, 0, cols, 1,
        "        <property name=\"hexpand\">true</property>\n",
    ));
    // Sidebar panel
    if content_rows > 0 {
        c.push_str(&child(
            "GtkLabel", "sidebar_label",
            0, 1, sidebar_cols, content_rows,
            "        <property name=\"label\">Sidebar</property>\n\
             <property name=\"valign\">start</property>\n",
        ));
        // Main scrolled text view
        c.push_str(&child(
            "GtkScrolledWindow", "text_scroll",
            sidebar_cols, 1, content_cols, content_rows,
            "        <property name=\"hexpand\">true</property>\n\
             <property name=\"vexpand\">true</property>\n",
        ));
    }
    grid_xml(rows, cols, &c)
}

/// Header + left panel + right panel.
fn two_panel(rows: i32, cols: i32) -> String {
    let rows = rows.max(2);
    let cols = cols.max(2);
    let left_cols  = cols / 2;
    let right_cols = cols - left_cols;
    let content_rows = rows - 1;

    let mut c = String::new();
    c.push_str(&child(
        "GtkHeaderBar", "header_bar",
        0, 0, cols, 1,
        "        <property name=\"hexpand\">true</property>\n",
    ));
    if content_rows > 0 {
        c.push_str(&child(
            "GtkFrame", "left_panel",
            0, 1, left_cols, content_rows,
            "        <property name=\"label\">Left Panel</property>\n\
             <property name=\"hexpand\">true</property>\n\
             <property name=\"vexpand\">true</property>\n",
        ));
        c.push_str(&child(
            "GtkFrame", "right_panel",
            left_cols, 1, right_cols, content_rows,
            "        <property name=\"label\">Right Panel</property>\n\
             <property name=\"hexpand\">true</property>\n\
             <property name=\"vexpand\">true</property>\n",
        ));
    }
    grid_xml(rows, cols, &c)
}

/// Score/title bar at top, large play area, controls row at bottom.
fn game_canvas(rows: i32, cols: i32) -> String {
    let rows = rows.max(3);
    let content_rows = (rows - 2).max(1);
    let mut c = String::new();
    c.push_str(&child(
        "GtkLabel", "score_bar",
        0, 0, cols, 1,
        "        <property name=\"label\">Score: 0</property>\n\
         <property name=\"xalign\">0.5</property>\n",
    ));
    c.push_str(&merged(0, 1, cols, content_rows));
    c.push_str(&child(
        "GtkBox", "controls_row",
        0, rows - 1, cols, 1,
        "        <property name=\"orientation\">horizontal</property>\n\
         <property name=\"spacing\">8</property>\n\
         <property name=\"halign\">center</property>\n",
    ));
    grid_xml(rows, cols, &c)
}

/// Header bar, large media view, seek bar + playback controls below.
fn media_player(rows: i32, cols: i32) -> String {
    let rows = rows.max(4);
    let media_rows = (rows - 3).max(1);
    let mut c = String::new();
    c.push_str(&child("GtkHeaderBar", "header_bar", 0, 0, cols, 1, "        <property name=\"hexpand\">true</property>\n"));
    c.push_str(&merged(0, 1, cols, media_rows));
    c.push_str(&child(
        "GtkScale", "seek_bar",
        0, rows - 2, cols, 1,
        "        <property name=\"orientation\">horizontal</property>\n\
         <property name=\"hexpand\">true</property>\n\
         <property name=\"draw-value\">false</property>\n",
    ));
    c.push_str(&child(
        "GtkBox", "playback_controls",
        0, rows - 1, cols, 1,
        "        <property name=\"orientation\">horizontal</property>\n\
         <property name=\"spacing\">12</property>\n\
         <property name=\"halign\">center</property>\n",
    ));
    grid_xml(rows, cols, &c)
}

/// Header bar, scrollable message list, input + send button row.
fn chat(rows: i32, cols: i32) -> String {
    let rows = rows.max(3);
    let list_rows = (rows - 2).max(1);
    let mut c = String::new();
    c.push_str(&child("GtkHeaderBar", "header_bar", 0, 0, cols, 1, "        <property name=\"hexpand\">true</property>\n"));
    c.push_str(&child(
        "GtkScrolledWindow", "message_scroll",
        0, 1, cols, list_rows,
        "        <property name=\"hexpand\">true</property>\n\
         <property name=\"vexpand\">true</property>\n",
    ));
    // Input row: entry takes most columns, send button takes last 1
    let entry_cols = (cols - 1).max(1);
    c.push_str(&child(
        "GtkEntry", "message_entry",
        0, rows - 1, entry_cols, 1,
        "        <property name=\"placeholder-text\">Type a message…</property>\n\
         <property name=\"hexpand\">true</property>\n",
    ));
    c.push_str(&child(
        "GtkButton", "send_button",
        entry_cols, rows - 1, 1, 1,
        "        <property name=\"label\">Send</property>\n",
    ));
    grid_xml(rows, cols, &c)
}

/// Header bar, path toolbar, folder tree + file list, status bar.
fn file_browser(rows: i32, cols: i32) -> String {
    let rows = rows.max(4);
    let cols = cols.max(2);
    let tree_cols  = (cols / 3).max(1);
    let files_cols = cols - tree_cols;
    let content_rows = (rows - 3).max(1);
    let mut c = String::new();
    c.push_str(&child("GtkHeaderBar", "header_bar", 0, 0, cols, 1, "        <property name=\"hexpand\">true</property>\n"));
    c.push_str(&child(
        "GtkEntry", "path_entry",
        0, 1, cols, 1,
        "        <property name=\"placeholder-text\">/home/user/…</property>\n\
         <property name=\"hexpand\">true</property>\n",
    ));
    c.push_str(&child(
        "GtkScrolledWindow", "folder_tree",
        0, 2, tree_cols, content_rows,
        "        <property name=\"vexpand\">true</property>\n",
    ));
    c.push_str(&child(
        "GtkScrolledWindow", "file_list",
        tree_cols, 2, files_cols, content_rows,
        "        <property name=\"hexpand\">true</property>\n\
         <property name=\"vexpand\">true</property>\n",
    ));
    c.push_str(&child(
        "GtkLabel", "status_bar",
        0, rows - 1, cols, 1,
        "        <property name=\"label\">0 items</property>\n\
         <property name=\"xalign\">0</property>\n",
    ));
    grid_xml(rows, cols, &c)
}

/// Header + 2×2 tile grid of content panels.
fn dashboard(rows: i32, cols: i32) -> String {
    let rows = rows.max(3);
    let cols = cols.max(2);
    let tile_rows = rows - 1;
    let top_rows  = tile_rows / 2;
    let bot_rows  = tile_rows - top_rows;
    let left_cols = cols / 2;
    let right_cols = cols - left_cols;

    let mut c = String::new();
    c.push_str(&child(
        "GtkHeaderBar", "header_bar",
        0, 0, cols, 1,
        "        <property name=\"hexpand\">true</property>\n",
    ));

    let tiles = [
        ("tile_tl", 0,          1,           left_cols,  top_rows, "Top Left"),
        ("tile_tr", left_cols,  1,           right_cols, top_rows, "Top Right"),
        ("tile_bl", 0,          1 + top_rows, left_cols, bot_rows, "Bottom Left"),
        ("tile_br", left_cols,  1 + top_rows, right_cols,bot_rows, "Bottom Right"),
    ];
    for (id, col, row, cs, rs, lbl) in &tiles {
        if *cs > 0 && *rs > 0 {
            c.push_str(&child(
                "GtkFrame", id, *col, *row, *cs, *rs,
                &format!(
                    "        <property name=\"label\">{lbl}</property>\n\
                     <property name=\"hexpand\">true</property>\n\
                     <property name=\"vexpand\">true</property>\n"
                ),
            ));
        }
    }
    grid_xml(rows, cols, &c)
}
