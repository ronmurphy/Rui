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
                icon: "⬇",
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
                icon: "➡",
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
                icon: "▦",
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
                icon: "▢",
                snippet: r#"<object class="GtkFrame">
  <property name="label">Frame Title</property>
  <child>
    <!-- Frame content -->
  </child>
</object>"#,
            },
            WidgetEntry {
                name: "ScrolledWindow",
                icon: "↕",
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
                icon: "⧉",
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
                icon: "📑",
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
                icon: "⬌",
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
        ],
    },
    Category {
        name: "Display",
        widgets: &[
            WidgetEntry {
                name: "Label",
                icon: "𝐓",
                snippet: r#"<object class="GtkLabel">
  <property name="label">Label text</property>
  <property name="halign">start</property>
</object>"#,
            },
            WidgetEntry {
                name: "Image",
                icon: "🖼",
                snippet: r#"<object class="GtkImage">
  <property name="icon-name">image-x-generic</property>
  <property name="pixel-size">48</property>
</object>"#,
            },
            WidgetEntry {
                name: "Separator",
                icon: "—",
                snippet: r#"<object class="GtkSeparator">
  <property name="orientation">horizontal</property>
</object>"#,
            },
            WidgetEntry {
                name: "ProgressBar",
                icon: "▰",
                snippet: r#"<object class="GtkProgressBar">
  <property name="fraction">0.5</property>
  <property name="show-text">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "Spinner",
                icon: "⟳",
                snippet: r#"<object class="GtkSpinner">
  <property name="spinning">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "LevelBar",
                icon: "▮",
                snippet: r#"<object class="GtkLevelBar">
  <property name="value">0.6</property>
  <property name="min-value">0</property>
  <property name="max-value">1</property>
</object>"#,
            },
        ],
    },
    Category {
        name: "Input",
        widgets: &[
            WidgetEntry {
                name: "Button",
                icon: "🔘",
                snippet: r#"<object class="GtkButton">
  <property name="label">Click Me</property>
</object>"#,
            },
            WidgetEntry {
                name: "ToggleButton",
                icon: "⊡",
                snippet: r#"<object class="GtkToggleButton">
  <property name="label">Toggle</property>
</object>"#,
            },
            WidgetEntry {
                name: "CheckButton",
                icon: "☑",
                snippet: r#"<object class="GtkCheckButton">
  <property name="label">Check me</property>
</object>"#,
            },
            WidgetEntry {
                name: "Switch",
                icon: "⏻",
                snippet: r#"<object class="GtkSwitch">
  <property name="active">false</property>
  <property name="halign">start</property>
</object>"#,
            },
            WidgetEntry {
                name: "Entry",
                icon: "⌨",
                snippet: r#"<object class="GtkEntry">
  <property name="placeholder-text">Type here…</property>
</object>"#,
            },
            WidgetEntry {
                name: "PasswordEntry",
                icon: "🔒",
                snippet: r#"<object class="GtkPasswordEntry">
  <property name="show-peek-icon">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "SpinButton",
                icon: "🔢",
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
                icon: "⟺",
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
                icon: "▾",
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
                icon: "⏷",
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
        ],
    },
    Category {
        name: "Text",
        widgets: &[
            WidgetEntry {
                name: "TextView",
                icon: "📝",
                snippet: r#"<object class="GtkTextView">
  <property name="editable">true</property>
  <property name="wrap-mode">word</property>
  <property name="hexpand">true</property>
  <property name="vexpand">true</property>
</object>"#,
            },
            WidgetEntry {
                name: "SearchEntry",
                icon: "🔍",
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
                icon: "▔",
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
                icon: "▁",
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
                icon: "▶",
                snippet: r#"<object class="GtkExpander">
  <property name="label">Details</property>
  <child>
    <object class="GtkLabel">
      <property name="label">Hidden content</property>
    </object>
  </child>
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

/// Insert an XML snippet at the cursor, wrapped in `<child>` tags,
/// and auto-indented to match the current cursor indentation.
fn insert_snippet(buffer: &sourceview5::Buffer, snippet: &str) {
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
