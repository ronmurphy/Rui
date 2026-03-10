//! palette.rs — Widget palette panel.
//!
//! Lists common GTK4 widgets grouped by category. Clicking a widget inserts
//! its XML snippet at the cursor position in the active editor buffer.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Label, Orientation, ScrolledWindow, Separator,
};
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A single widget template in the palette.
struct WidgetEntry {
    name: &'static str,
    icon: &'static str,      // Nerd Font or emoji hint
    snippet: &'static str,   // XML to insert
}

/// Categories of widgets.
struct Category {
    name: &'static str,
    widgets: &'static [WidgetEntry],
}

const CATEGORIES: &[Category] = &[
    Category {
        name: "Containers",
        widgets: &[
            WidgetEntry {
                name: "Box (Vertical)",
                icon: "\u{F063}",  // nf-fa-arrow_down
                snippet: r#"<object class="GtkBox">
  <property name="orientation">vertical</property>
  <property name="spacing">8</property>
  <child>
    <!-- Add children here -->
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Box (Horizontal)",
                icon: "\u{F061}",  // nf-fa-arrow_right
                snippet: r#"<object class="GtkBox">
  <property name="orientation">horizontal</property>
  <property name="spacing">8</property>
  <child>
    <!-- Add children here -->
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Grid",
                icon: "\u{F00A}",  // nf-fa-th
                snippet: r#"<object class="GtkGrid">
  <property name="row-spacing">8</property>
  <property name="column-spacing">8</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Cell</property>
      <layout>
        <property name="column">0</property>
        <property name="row">0</property>
      </layout>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Frame",
                icon: "\u{F2D2}",  // nf-fa-window_maximize
                snippet: r#"<object class="GtkFrame">
  <property name="label">Frame Title</property>
  <child>
    <!-- Frame content -->
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "ScrolledWindow",
                icon: "\u{F07D}",  // nf-fa-arrows_v
                snippet: r#"<object class="GtkScrolledWindow">
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
  <child>
    <!-- Scrollable content -->
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Stack",
                icon: "\u{F24D}",  // nf-fa-clone
                snippet: r#"<object class="GtkStack" id="my_stack">
  <child>
    <object class="GtkStackPage">
      <property name="name">page1</property>
      <property name="title">Page 1</property>
      <property name="child">
        <object class="GtkLabel">
          <property name="label">Page 1 content</property>
        </object>
      </property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Notebook",
                icon: "\u{F2BB}",  // nf-oct-browser (tabbed view)
                snippet: r#"<object class="GtkNotebook">
  <child>
    <object class="GtkNotebookPage">
      <property name="tab-label">Tab 1</property>
      <property name="child">
        <object class="GtkLabel">
          <property name="label">Tab content</property>
        </object>
      </property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Paned (Horizontal)",
                icon: "\u{F07E}",  // nf-fa-arrows_h
                snippet: r#"<object class="GtkPaned">
  <property name="orientation">horizontal</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Left</property>
    </object>
  </child>
  <child>
    <object class="GtkLabel">
      <property name="label">Right</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Paned (Vertical)",
                icon: "\u{F07D}",  // nf-fa-arrows_v
                snippet: r#"<object class="GtkPaned">
  <property name="orientation">vertical</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Top</property>
    </object>
  </child>
  <child>
    <object class="GtkLabel">
      <property name="label">Bottom</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "ListBox",
                icon: "\u{F03A}",  // nf-fa-list
                snippet: r#"<object class="GtkListBox">
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
</object>"#,
            },
            WidgetEntry {
                name: "FlowBox",
                icon: "\u{F009}",  // nf-fa-th_large
                snippet: r#"<object class="GtkFlowBox">
  <property name="max-children-per-line">4</property>
  <property name="selection-mode">none</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Item 1</property>
    </object>
  </child>
  <child>
    <object class="GtkLabel">
      <property name="label">Item 2</property>
    </object>
  </child>
  <child>
    <object class="GtkLabel">
      <property name="label">Item 3</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Revealer",
                icon: "\u{F070}",  // nf-fa-eye_slash
                snippet: r#"<object class="GtkRevealer">
  <property name="transition-type">slide-down</property>
  <property name="reveal-child">true</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Revealed content</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Overlay",
                icon: "\u{F1B2}",  // nf-fa-cube
                snippet: r#"<object class="GtkOverlay">
  <child>
    <object class="GtkLabel">
      <property name="label">Base layer</property>
    </object>
  </child>
  <child type="overlay">
    <object class="GtkLabel">
      <property name="label">Overlay</property>
      <property name="halign">end</property>
      <property name="valign">end</property>
      <property name="margin-end">8</property>
      <property name="margin-bottom">8</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "AspectFrame",
                icon: "\u{F096}",  // nf-fa-square_o
                snippet: r#"<object class="GtkAspectFrame">
  <property name="ratio">1.777</property>
  <child>
    <object class="GtkLabel">
      <property name="label">16:9 area</property>
    </object>
  </child>
</object>"#,
            },
        ],
    },
    Category {
        name: "Display",
        widgets: &[
            WidgetEntry {
                name: "Label",
                icon: "\u{F031}",  // nf-fa-font
                snippet: r#"<object class="GtkLabel">
  <property name="label">Label text</property>
  <property name="halign">start</property>
</object>"#,
            },
            WidgetEntry {
                name: "Image",
                icon: "\u{F03E}",  // nf-fa-picture_o
                snippet: r#"<object class="GtkImage">
  <property name="icon-name">image-x-generic</property>
  <property name="pixel-size">48</property>
</object>"#,
            },
            WidgetEntry {
                name: "Separator",
                icon: "\u{F068}",  // nf-fa-minus (horizontal rule)
                snippet: r#"<object class="GtkSeparator">
  <property name="orientation">horizontal</property>
</object>"#,
            },
            WidgetEntry {
                name: "ProgressBar",
                icon: "\u{F242}",  // nf-fa-battery_half
                snippet: r#"<object class="GtkProgressBar">
  <property name="fraction">0.5</property>
  <property name="show-text">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "Spinner",
                icon: "\u{F110}",  // nf-fa-spinner
                snippet: r#"<object class="GtkSpinner">
  <property name="spinning">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "LevelBar",
                icon: "\u{F241}",  // nf-fa-battery_quarter
                snippet: r#"<object class="GtkLevelBar">
  <property name="value">0.6</property>
  <property name="min-value">0</property>
  <property name="max-value">1</property>
</object>"#,
            },
            WidgetEntry {
                name: "Picture",
                icon: "\u{F1C5}",  // nf-fa-file_image_o
                snippet: r#"<object class="GtkPicture">
  <property name="content-fit">contain</property>
  <property name="width-request">200</property>
  <property name="height-request">150</property>
</object>"#,
            },
            WidgetEntry {
                name: "Calendar",
                icon: "\u{F073}",  // nf-fa-calendar
                snippet: r#"<object class="GtkCalendar">
  <property name="show-heading">true</property>
  <property name="show-day-names">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "DrawingArea",
                icon: "\u{F044}",  // nf-fa-pencil_square_o
                snippet: r#"<object class="GtkDrawingArea">
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
  <property name="width-request">200</property>
  <property name="height-request">150</property>
</object>"#,
            },
            WidgetEntry {
                name: "Video",
                icon: "\u{F03D}",  // nf-fa-film
                snippet: r#"<object class="GtkVideo">
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "GLArea",
                icon: "\u{F1B2}",  // nf-fa-cube
                snippet: r#"<object class="GtkGLArea">
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
  <property name="width-request">200</property>
  <property name="height-request">150</property>
</object>"#,
            },
        ],
    },
    Category {
        name: "Input",
        widgets: &[
            WidgetEntry {
                name: "Button",
                icon: "\u{F25A}",  // nf-fa-hand_pointer_o (clickable)
                snippet: r#"<object class="GtkButton">
  <property name="label">Click Me</property>
</object>"#,
            },
            WidgetEntry {
                name: "ToggleButton",
                icon: "\u{F204}",  // nf-fa-toggle_off
                snippet: r#"<object class="GtkToggleButton">
  <property name="label">Toggle</property>
</object>"#,
            },
            WidgetEntry {
                name: "CheckButton",
                icon: "\u{F14A}",  // nf-fa-check_square
                snippet: r#"<object class="GtkCheckButton">
  <property name="label">Check me</property>
</object>"#,
            },
            WidgetEntry {
                name: "Switch",
                icon: "\u{F205}",  // nf-fa-toggle_on
                snippet: r#"<object class="GtkSwitch">
  <property name="active">false</property>
  <property name="halign">start</property>
</object>"#,
            },
            WidgetEntry {
                name: "Entry",
                icon: "\u{F11C}",  // nf-fa-keyboard_o
                snippet: r#"<object class="GtkEntry">
  <property name="placeholder-text">Type here…</property>
</object>"#,
            },
            WidgetEntry {
                name: "PasswordEntry",
                icon: "\u{F023}",  // nf-fa-lock
                snippet: r#"<object class="GtkPasswordEntry">
  <property name="show-peek-icon">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "SpinButton",
                icon: "\u{F162}",  // nf-fa-sort_numeric_asc
                snippet: r#"<object class="GtkSpinButton">
  <property name="adjustment">
    <object class="GtkAdjustment">
      <property name="lower">0</property>
      <property name="upper">100</property>
      <property name="step-increment">1</property>
      <property name="value">50</property>
    </object>
  </property>
</object>"#,
            },
            WidgetEntry {
                name: "Scale",
                icon: "\u{F1DE}",  // nf-fa-sliders
                snippet: r#"<object class="GtkScale">
  <property name="orientation">horizontal</property>
  <property name="hexpand">true</property>
  <property name="adjustment">
    <object class="GtkAdjustment">
      <property name="lower">0</property>
      <property name="upper">100</property>
      <property name="step-increment">1</property>
      <property name="value">50</property>
    </object>
  </property>
  <property name="draw-value">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "ComboBoxText",
                icon: "\u{F0D7}",  // nf-fa-caret_down
                snippet: r#"<object class="GtkComboBoxText">
  <items>
    <item>Option 1</item>
    <item>Option 2</item>
    <item>Option 3</item>
  </items>
</object>"#,
            },
            WidgetEntry {
                name: "DropDown",
                icon: "\u{F150}",  // nf-fa-caret_square_o_down
                snippet: r#"<object class="GtkDropDown">
  <property name="model">
    <object class="GtkStringList">
      <items>
        <item>Option 1</item>
        <item>Option 2</item>
        <item>Option 3</item>
      </items>
    </object>
  </property>
</object>"#,
            },
            WidgetEntry {
                name: "MenuButton",
                icon: "\u{F0C9}",  // nf-fa-bars (hamburger menu)
                snippet: r#"<object class="GtkMenuButton">
  <property name="icon-name">open-menu-symbolic</property>
  <property name="tooltip-text">Menu</property>
</object>"#,
            },
            WidgetEntry {
                name: "LinkButton",
                icon: "\u{F0C1}",  // nf-fa-link
                snippet: r#"<object class="GtkLinkButton">
  <property name="label">Visit website</property>
  <property name="uri">https://example.com</property>
</object>"#,
            },
            WidgetEntry {
                name: "ColorDialogButton",
                icon: "\u{F1FC}",  // nf-fa-paint_brush
                snippet: r#"<object class="GtkColorDialogButton">
  <property name="dialog">
    <object class="GtkColorDialog">
      <property name="with-alpha">false</property>
    </object>
  </property>
</object>"#,
            },
            WidgetEntry {
                name: "FontDialogButton",
                icon: "\u{F035}",  // nf-fa-text_height
                snippet: r#"<object class="GtkFontDialogButton">
  <property name="dialog">
    <object class="GtkFontDialog">
      <property name="title">Choose Font</property>
    </object>
  </property>
</object>"#,
            },
        ],
    },
    Category {
        name: "Text",
        widgets: &[
            WidgetEntry {
                name: "TextView",
                icon: "\u{F15C}",  // nf-fa-file_text
                snippet: r#"<object class="GtkTextView">
  <property name="editable">true</property>
  <property name="wrap-mode">word</property>
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "SearchEntry",
                icon: "\u{F002}",  // nf-fa-search
                snippet: r#"<object class="GtkSearchEntry">
  <property name="placeholder-text">Search…</property>
</object>"#,
            },
        ],
    },
    Category {
        name: "Layout",
        widgets: &[
            WidgetEntry {
                name: "HeaderBar",
                icon: "\u{F2D2}",  // nf-fa-window_maximize (title bar feel)
                snippet: r#"<object class="GtkHeaderBar">
  <child type="start">
    <object class="GtkButton">
      <property name="icon-name">go-previous-symbolic</property>
    </object>
  </child>
  <child type="end">
    <object class="GtkButton">
      <property name="icon-name">open-menu-symbolic</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "ActionBar",
                icon: "\u{F2D3}",  // nf-fa-window_minimize (bottom bar feel)
                snippet: r#"<object class="GtkActionBar">
  <child type="start">
    <object class="GtkButton">
      <property name="label">Cancel</property>
    </object>
  </child>
  <child type="end">
    <object class="GtkButton">
      <property name="label">OK</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "Expander",
                icon: "\u{F054}",  // nf-fa-chevron_right (expandable)
                snippet: r#"<object class="GtkExpander">
  <property name="label">Details</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Hidden content</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "WindowHandle",
                icon: "\u{F047}",  // nf-fa-expand (draggable area)
                snippet: r#"<object class="GtkWindowHandle">
  <child>
    <object class="GtkLabel">
      <property name="label">Drag to move window</property>
      <property name="halign">center</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "CenterBox",
                icon: "\u{F0B2}",  // nf-fa-arrows (center layout)
                snippet: r#"<object class="GtkCenterBox">
  <child type="start">
    <object class="GtkLabel">
      <property name="label">Start</property>
    </object>
  </child>
  <child type="center">
    <object class="GtkLabel">
      <property name="label">Center</property>
    </object>
  </child>
  <child type="end">
    <object class="GtkLabel">
      <property name="label">End</property>
    </object>
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "StackSwitcher",
                icon: "\u{F009}",  // nf-fa-th_large (tab switcher)
                snippet: r#"<object class="GtkStackSwitcher">
  <property name="stack">my_stack</property>
</object>"#,
            },
            WidgetEntry {
                name: "StackSidebar",
                icon: "\u{F03A}",  // nf-fa-list (sidebar nav)
                snippet: r#"<object class="GtkStackSidebar">
  <property name="stack">my_stack</property>
</object>"#,
            },
        ],
    },
];

/// The widget palette panel.
#[derive(Clone)]
pub struct Palette {
    pub widget: GtkBox,
    /// The buffer to insert snippets into (updated on tab switch).
    target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>>,
}

impl Palette {
    pub fn new() -> Self {
        let target_buffer: Rc<RefCell<Option<sourceview5::Buffer>>> =
            Rc::new(RefCell::new(None));

        let header = Label::new(Some("Widgets"));
        header.set_halign(gtk4::Align::Start);
        header.set_margin_start(8);
        header.set_margin_top(6);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let list = GtkBox::new(Orientation::Vertical, 0);
        list.set_margin_start(4);
        list.set_margin_end(4);

        for category in CATEGORIES {
            // Category header
            let cat_label = Label::new(Some(category.name));
            cat_label.set_halign(gtk4::Align::Start);
            cat_label.set_margin_start(8);
            cat_label.set_margin_top(8);
            cat_label.set_margin_bottom(2);
            cat_label.add_css_class("dim-label");
            list.append(&cat_label);

            for entry in category.widgets {
                let btn = Button::new();
                let btn_label = Label::new(Some(&format!("{} {}", entry.icon, entry.name)));
                btn_label.set_halign(gtk4::Align::Start);
                btn_label.add_css_class("nf");
                btn.set_child(Some(&btn_label));
                btn.add_css_class("flat");
                btn.set_tooltip_text(Some(entry.name));

                let snippet = entry.snippet;
                let buf_ref = target_buffer.clone();

                btn.connect_clicked(move |_| {
                    let buf_opt = buf_ref.borrow();
                    match buf_opt.as_ref() {
                        Some(buf) => {
                            insert_snippet(buf, snippet);
                        }
                        None => {
                            log::warn!("Palette: no buffer connected");
                        }
                    }
                });

                list.append(&btn);
            }

            let sep = Separator::new(Orientation::Horizontal);
            sep.set_margin_top(4);
            sep.set_margin_bottom(2);
            list.append(&sep);
        }

        let scroll = ScrolledWindow::builder()
            .hexpand(false)
            .vexpand(true)
            .child(&list)
            .build();

        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_width_request(200);
        widget.append(&header);
        widget.append(&scroll);

        Palette {
            widget,
            target_buffer,
        }
    }

    /// Set which buffer receives inserted snippets (call on tab switch).
    pub fn set_buffer(&self, buffer: &sourceview5::Buffer) {
        *self.target_buffer.borrow_mut() = Some(buffer.clone());
    }

    /// Clear the target buffer reference.
    pub fn clear_buffer(&self) {
        *self.target_buffer.borrow_mut() = None;
    }

    pub fn toggle(&self) {
        self.widget.set_visible(!self.widget.is_visible());
    }
}

/// Insert a widget snippet as a `<child>` of the container surrounding the cursor.
/// Falls back to raw cursor insertion if no container is found.
fn insert_snippet(buffer: &sourceview5::Buffer, snippet: &str) {
    let (buf_start, buf_end) = buffer.bounds();
    let full_text = buffer.text(&buf_start, &buf_end, false).to_string();
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let cursor_char = cursor.offset() as usize;

    // Convert char offset to byte offset
    let cursor_byte = full_text
        .char_indices()
        .nth(cursor_char)
        .map(|(i, _)| i)
        .unwrap_or(full_text.len());

    // Try to find the nearest container and insert before its </object>
    if let Some(insert_byte) = find_container_insert_point(&full_text, cursor_byte) {
        // Determine indentation from the </object> line
        let before_close = &full_text[..insert_byte];
        let last_nl = before_close.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let close_line = &full_text[last_nl..insert_byte];
        let base_indent: String = close_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();

        let mut result = String::new();
        result.push_str(&format!("  {}<child>\n", base_indent));
        for line in snippet.lines() {
            result.push_str(&format!("  {}  {}\n", base_indent, line));
        }
        result.push_str(&format!("  {}</child>\n", base_indent));

        let insert_char = full_text[..insert_byte].chars().count();
        let mut iter = buffer.iter_at_offset(insert_char as i32);
        buffer.begin_user_action();
        buffer.insert(&mut iter, &result);
        buffer.end_user_action();
    } else {
        // Fallback: insert at cursor with local indentation
        insert_snippet_at_cursor(buffer, snippet);
    }
}

/// Fallback: insert snippet wrapped in `<child>` at the current cursor position.
fn insert_snippet_at_cursor(buffer: &sourceview5::Buffer, snippet: &str) {
    let cursor = buffer.iter_at_mark(&buffer.get_insert());

    // Figure out the indentation at the cursor line
    let line = cursor.line();
    let line_start = buffer.iter_at_line(line).unwrap_or(cursor);
    let mut line_end = line_start;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }
    let line_text = buffer.text(&line_start, &line_end, false).to_string();
    let indent: String = line_text.chars().take_while(|c| c.is_whitespace()).collect();

    // Wrap in <child> and indent each line of the snippet
    let mut result = String::new();
    result.push_str(&format!("{}<child>\n", indent));
    for line in snippet.lines() {
        result.push_str(&format!("{}  {}\n", indent, line));
    }
    result.push_str(&format!("{}</child>\n", indent));

    buffer.begin_user_action();
    buffer.insert(&mut buffer.iter_at_mark(&buffer.get_insert()), &result);
    buffer.end_user_action();
}

// ─────────────────────────────────────────────────────────────────────
// XML helpers for smart insertion
// ─────────────────────────────────────────────────────────────────────

/// Find the byte offset where a new `<child>` should be inserted,
/// inside the nearest enclosing container `<object>`.
/// Returns the byte offset of the `</object>` closing tag.
fn find_container_insert_point(xml: &str, cursor_byte: usize) -> Option<usize> {
    let safe_cursor = cursor_byte.min(xml.len());
    let mut search_from = safe_cursor;

    loop {
        let before = &xml[..search_from];
        let obj_start = before.rfind("<object")?;

        // Extract class from the <object ...> tag
        let tag_end_rel = xml[obj_start..].find('>')?;
        let tag = &xml[obj_start..obj_start + tag_end_rel + 1];
        let class = extract_class_from_tag(tag);

        // Find matching </object>
        let remainder = &xml[obj_start..];
        let close_end = find_matching_close_object(remainder)?;
        let obj_close_abs = obj_start + close_end;

        // Check cursor is inside this object
        if safe_cursor <= obj_close_abs {
            if let Some(cls) = &class {
                if is_container_class(cls) {
                    // Insert point: right before the </object> closing tag
                    let close_tag_rel = remainder[..close_end].rfind("</object>")?;
                    return Some(obj_start + close_tag_rel);
                }
            }
        }

        // Not a container or cursor outside — try parent
        if obj_start == 0 {
            return None;
        }
        search_from = obj_start;
    }
}

/// Find the end of the matching `</object>` (byte offset relative to input start).
fn find_matching_close_object(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut pos = 0;
    while pos < s.len() {
        if s[pos..].starts_with("<object") {
            depth += 1;
            pos += 7;
        } else if s[pos..].starts_with("</object>") {
            depth -= 1;
            if depth == 0 {
                return Some(pos + "</object>".len());
            }
            pos += 9;
        } else {
            let ch_len = s[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            pos += ch_len;
        }
    }
    None
}

/// Extract the `class` attribute from an `<object ...>` tag string.
fn extract_class_from_tag(tag: &str) -> Option<String> {
    let start = tag.find("class=\"")? + "class=\"".len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// Check if a GTK class is a container that accepts `<child>` elements.
fn is_container_class(class: &str) -> bool {
    matches!(
        class,
        "GtkBox"
            | "GtkGrid"
            | "GtkFrame"
            | "GtkScrolledWindow"
            | "GtkPaned"
            | "GtkNotebook"
            | "GtkStack"
            | "GtkOverlay"
            | "GtkCenterBox"
            | "GtkExpander"
            | "GtkFlowBox"
            | "GtkListBox"
            | "GtkHeaderBar"
            | "GtkActionBar"
            | "GtkWindow"
            | "GtkApplicationWindow"
            | "GtkDialog"
            | "GtkPopover"
            | "GtkRevealer"
            | "GtkViewport"
    )
}
