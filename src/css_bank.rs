use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SelectionMode, TextView, Window, WrapMode,
};
use std::cell::RefCell;
use std::rc::Rc;

struct Snippet {
    name: &'static str,
    category: &'static str,
    description: &'static str,
    css: &'static str,
}

const SNIPPETS: &[Snippet] = &[
    // ── Themes ─────────────────────────────────────────────────────────────────
    Snippet {
        name: "Dark Surface",
        category: "Themes",
        description: "Dark window with card-style surfaces",
        css: "/* Dark surface theme */\nwindow, .background {\n    background-color: #1e1e2e;\n    color: #cdd6f4;\n}\n\n.card, frame {\n    background-color: #313244;\n    border-radius: 12px;\n    border: 1px solid #45475a;\n    padding: 12px;\n}\n\nbutton {\n    background-color: #45475a;\n    color: #cdd6f4;\n    border: none;\n    border-radius: 8px;\n}\n\nbutton:hover {\n    background-color: #585b70;\n}\n",
    },
    Snippet {
        name: "Nord Inspired",
        category: "Themes",
        description: "Cool blue-grey palette inspired by Nord",
        css: "/* Nord-inspired theme */\nwindow, .background {\n    background-color: #2e3440;\n    color: #d8dee9;\n}\n\n.card, frame {\n    background-color: #3b4252;\n    border-radius: 10px;\n    border: 1px solid #434c5e;\n    padding: 12px;\n}\n\nbutton {\n    background-color: #5e81ac;\n    color: #eceff4;\n    border: none;\n    border-radius: 6px;\n}\n\nbutton:hover {\n    background-color: #81a1c1;\n}\n",
    },
    Snippet {
        name: "High Contrast",
        category: "Themes",
        description: "Accessible high-contrast black and white",
        css: "/* High contrast theme */\nwindow, .background {\n    background-color: #000000;\n    color: #ffffff;\n}\n\nbutton {\n    background-color: #ffffff;\n    color: #000000;\n    border: 2px solid #ffffff;\n    border-radius: 4px;\n}\n\nbutton:hover {\n    background-color: #ffff00;\n    color: #000000;\n}\n\nentry {\n    background-color: #000000;\n    color: #ffffff;\n    border: 2px solid #ffffff;\n}\n",
    },
    Snippet {
        name: "Warm Sepia",
        category: "Themes",
        description: "Warm off-white background with brown tones",
        css: "/* Warm sepia theme */\nwindow, .background {\n    background-color: #f5f0e8;\n    color: #3d2b1f;\n}\n\n.card, frame {\n    background-color: #ede8dc;\n    border-radius: 10px;\n    border: 1px solid #c8b8a2;\n    padding: 12px;\n}\n\nbutton {\n    background-color: #8b6342;\n    color: #f5f0e8;\n    border: none;\n    border-radius: 6px;\n}\n\nbutton:hover {\n    background-color: #a0774f;\n}\n",
    },
    // ── Buttons ────────────────────────────────────────────────────────────────
    Snippet {
        name: "Pill Button",
        category: "Buttons",
        description: "Fully rounded pill-shaped button",
        css: "/* Pill button — add class=\"pill\" to your GtkButton */\nbutton.pill {\n    border-radius: 999px;\n    padding: 8px 20px;\n    background-color: #3584e4;\n    color: white;\n    font-weight: bold;\n    border: none;\n    box-shadow: 0 2px 4px rgba(0,0,0,0.25);\n    transition: all 200ms ease;\n}\n\nbutton.pill:hover {\n    background-color: #4a9cf5;\n    box-shadow: 0 4px 8px rgba(0,0,0,0.3);\n}\n\nbutton.pill:active {\n    box-shadow: 0 1px 2px rgba(0,0,0,0.2);\n}\n",
    },
    Snippet {
        name: "Ghost Button",
        category: "Buttons",
        description: "Transparent button with visible border",
        css: "/* Ghost button — add class=\"ghost\" to your GtkButton */\nbutton.ghost {\n    background-color: transparent;\n    color: #3584e4;\n    border: 2px solid #3584e4;\n    border-radius: 8px;\n    padding: 6px 16px;\n    transition: all 180ms ease;\n}\n\nbutton.ghost:hover {\n    background-color: rgba(53, 132, 228, 0.12);\n}\n\nbutton.ghost:active {\n    background-color: rgba(53, 132, 228, 0.24);\n}\n",
    },
    Snippet {
        name: "Danger Button",
        category: "Buttons",
        description: "Red destructive action button",
        css: "/* Danger button — add class=\"danger\" to your GtkButton */\nbutton.danger {\n    background-color: #e01b24;\n    color: white;\n    border: none;\n    border-radius: 8px;\n    padding: 8px 16px;\n    font-weight: bold;\n    transition: background-color 150ms ease;\n}\n\nbutton.danger:hover {\n    background-color: #c01c25;\n}\n\nbutton.danger:active {\n    background-color: #a01321;\n}\n",
    },
    Snippet {
        name: "Success Button",
        category: "Buttons",
        description: "Green confirmation or success button",
        css: "/* Success button — add class=\"success\" to your GtkButton */\nbutton.success {\n    background-color: #26a269;\n    color: white;\n    border: none;\n    border-radius: 8px;\n    padding: 8px 16px;\n    font-weight: bold;\n    transition: background-color 150ms ease;\n}\n\nbutton.success:hover {\n    background-color: #2ec27e;\n}\n\nbutton.success:active {\n    background-color: #1c8950;\n}\n",
    },
    Snippet {
        name: "Icon + Label Button",
        category: "Buttons",
        description: "Flat button with icon leading a label",
        css: "/* Icon + label button */\nbutton.icon-label {\n    background-color: transparent;\n    border: none;\n    border-radius: 8px;\n    padding: 6px 12px;\n    transition: background-color 150ms ease;\n}\n\nbutton.icon-label:hover {\n    background-color: rgba(0, 0, 0, 0.08);\n}\n\nbutton.icon-label label {\n    font-size: 13px;\n}\n",
    },
    // ── Cards ──────────────────────────────────────────────────────────────────
    Snippet {
        name: "Raised Card",
        category: "Cards",
        description: "Card with drop shadow and elevation",
        css: "/* Raised card — add class=\"card\" to a GtkFrame or GtkBox */\n.card {\n    background-color: #ffffff;\n    border-radius: 12px;\n    border: none;\n    box-shadow:\n        0 1px 3px rgba(0,0,0,0.12),\n        0 4px 12px rgba(0,0,0,0.08);\n    padding: 16px;\n    transition: box-shadow 200ms ease;\n}\n\n.card:hover {\n    box-shadow:\n        0 2px 8px rgba(0,0,0,0.15),\n        0 8px 24px rgba(0,0,0,0.12);\n}\n",
    },
    Snippet {
        name: "Glass Card",
        category: "Cards",
        description: "Frosted glass semi-transparent panel",
        css: "/* Glass / frosted card — add class=\"glass\" */\n.glass {\n    background-color: rgba(255, 255, 255, 0.12);\n    border-radius: 16px;\n    border: 1px solid rgba(255, 255, 255, 0.25);\n    box-shadow:\n        0 4px 24px rgba(0,0,0,0.15),\n        inset 0 1px 0 rgba(255,255,255,0.2);\n    padding: 20px;\n}\n",
    },
    Snippet {
        name: "Inset Well",
        category: "Cards",
        description: "Recessed inset panel with inner shadow",
        css: "/* Inset well — add class=\"well\" */\n.well {\n    background-color: rgba(0,0,0,0.06);\n    border-radius: 8px;\n    border: 1px solid rgba(0,0,0,0.10);\n    box-shadow: inset 0 2px 4px rgba(0,0,0,0.08);\n    padding: 12px;\n}\n",
    },
    Snippet {
        name: "Notification Banner",
        category: "Cards",
        description: "Coloured info/warning/error banner strip",
        css: "/* Notification banners */\n.banner-info {\n    background-color: #1c71d8;\n    color: white;\n    border-radius: 8px;\n    padding: 10px 14px;\n}\n\n.banner-warning {\n    background-color: #e5a50a;\n    color: #1c1c1e;\n    border-radius: 8px;\n    padding: 10px 14px;\n}\n\n.banner-error {\n    background-color: #e01b24;\n    color: white;\n    border-radius: 8px;\n    padding: 10px 14px;\n}\n\n.banner-success {\n    background-color: #26a269;\n    color: white;\n    border-radius: 8px;\n    padding: 10px 14px;\n}\n",
    },
    // ── Typography ─────────────────────────────────────────────────────────────
    Snippet {
        name: "Display Heading",
        category: "Typography",
        description: "Large bold display title and subtitle",
        css: "/* Display heading — add class=\"display-title\" / \"display-subtitle\" */\n.display-title {\n    font-size: 32px;\n    font-weight: 800;\n    letter-spacing: -0.5px;\n    color: #1c1c1e;\n}\n\n.display-subtitle {\n    font-size: 18px;\n    font-weight: 400;\n    color: #6c6c70;\n    margin-top: 4px;\n}\n",
    },
    Snippet {
        name: "Monospace Panel",
        category: "Typography",
        description: "Code-style monospace text panel",
        css: "/* Monospace / code panel — add class=\"code-panel\" */\n.code-panel {\n    font-family: monospace;\n    font-size: 13px;\n    background-color: #1e1e1e;\n    color: #d4d4d4;\n    border-radius: 8px;\n    padding: 12px 16px;\n    border: 1px solid #333;\n}\n\n.code-panel label {\n    font-family: monospace;\n    color: inherit;\n}\n",
    },
    Snippet {
        name: "Muted Labels & Badges",
        category: "Typography",
        description: "Secondary text, captions, and inline badges",
        css: "/* Muted labels and badges */\n.muted {\n    color: rgba(0, 0, 0, 0.45);\n    font-size: 12px;\n}\n\n.caption {\n    font-size: 11px;\n    font-weight: 600;\n    letter-spacing: 0.5px;\n    text-transform: uppercase;\n    color: rgba(0, 0, 0, 0.35);\n}\n\n.badge {\n    background-color: rgba(0,0,0,0.10);\n    border-radius: 99px;\n    padding: 2px 8px;\n    font-size: 11px;\n    font-weight: 700;\n}\n\n.badge-accent {\n    background-color: #3584e4;\n    color: white;\n    border-radius: 99px;\n    padding: 2px 8px;\n    font-size: 11px;\n    font-weight: 700;\n}\n",
    },
    // ── Animations ─────────────────────────────────────────────────────────────
    Snippet {
        name: "Fade In",
        category: "Animations",
        description: "Smooth opacity fade-in on widget",
        css: "/* Fade in — add class=\"fade-in\" */\n@keyframes fade-in {\n    from { opacity: 0; }\n    to   { opacity: 1; }\n}\n\n.fade-in {\n    animation: fade-in 300ms ease forwards;\n}\n",
    },
    Snippet {
        name: "Bounce",
        category: "Animations",
        description: "Bouncy scale-up entrance",
        css: "/* Bounce — add class=\"bounce\" */\n@keyframes bounce {\n    0%   { transform: scale(0.85); opacity: 0; }\n    60%  { transform: scale(1.06); opacity: 1; }\n    80%  { transform: scale(0.97); }\n    100% { transform: scale(1.00); }\n}\n\n.bounce {\n    animation: bounce 420ms cubic-bezier(0.34, 1.56, 0.64, 1) forwards;\n}\n",
    },
    Snippet {
        name: "Shake",
        category: "Animations",
        description: "Horizontal shake for error feedback",
        css: "/* Shake — add class=\"shake\" for form errors */\n@keyframes shake {\n    0%, 100% { transform: translateX(0);   }\n    15%      { transform: translateX(-8px); }\n    30%      { transform: translateX(6px);  }\n    45%      { transform: translateX(-6px); }\n    60%      { transform: translateX(4px);  }\n    75%      { transform: translateX(-3px); }\n    90%      { transform: translateX(2px);  }\n}\n\n.shake {\n    animation: shake 500ms ease;\n}\n",
    },
    Snippet {
        name: "Slide In Left",
        category: "Animations",
        description: "Panel slides in from the left edge",
        css: "/* Slide in from left — add class=\"slide-in-left\" */\n@keyframes slide-in-left {\n    from { transform: translateX(-100%); opacity: 0; }\n    to   { transform: translateX(0);     opacity: 1; }\n}\n\n.slide-in-left {\n    animation: slide-in-left 260ms ease-out forwards;\n}\n",
    },
    Snippet {
        name: "Wobble",
        category: "Animations",
        description: "Playful rotation wobble for attention",
        css: "/* Wobble — add class=\"wobble\" */\n@keyframes wobble {\n    0%   { transform: rotate(0deg);  }\n    15%  { transform: rotate(-8deg); }\n    30%  { transform: rotate(6deg);  }\n    45%  { transform: rotate(-5deg); }\n    60%  { transform: rotate(3deg);  }\n    75%  { transform: rotate(-2deg); }\n    100% { transform: rotate(0deg);  }\n}\n\n.wobble {\n    animation: wobble 600ms ease;\n    transform-origin: center bottom;\n}\n",
    },
    Snippet {
        name: "Pulse Glow",
        category: "Animations",
        description: "Repeating glow pulse for attention / loading",
        css: "/* Pulse glow — add class=\"pulse\" */\n@keyframes pulse {\n    0%, 100% { box-shadow: 0 0 0 0   rgba(53, 132, 228, 0.6); }\n    50%      { box-shadow: 0 0 0 8px rgba(53, 132, 228, 0.0); }\n}\n\n.pulse {\n    border-radius: 8px;\n    animation: pulse 1.6s ease infinite;\n}\n",
    },
];

const CATEGORIES: &[&str] = &["All", "Themes", "Buttons", "Cards", "Typography", "Animations"];

pub fn show_css_bank(
    parent: &gtk4::ApplicationWindow,
    notebook: &crate::notebook::NotebookManager,
) {
    let dialog = Window::builder()
        .title("CSS Bank")
        .transient_for(parent)
        .modal(true)
        .destroy_with_parent(true)
        .default_width(700)
        .default_height(580)
        .build();

    let outer = GtkBox::new(Orientation::Vertical, 0);

    // ── Category filter bar ────────────────────────────────────────────────────
    let cat_bar = GtkBox::new(Orientation::Horizontal, 4);
    cat_bar.set_margin_start(12);
    cat_bar.set_margin_end(12);
    cat_bar.set_margin_top(12);
    cat_bar.set_margin_bottom(8);

    // ── Snippet list ───────────────────────────────────────────────────────────
    let listbox = ListBox::new();
    listbox.set_selection_mode(SelectionMode::Single);

    let list_scroll = ScrolledWindow::builder()
        .child(&listbox)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(260)
        .build();

    // ── CSS preview ────────────────────────────────────────────────────────────
    let preview = TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(WrapMode::None)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(12)
        .right_margin(12)
        .hexpand(true)
        .build();

    let preview_scroll = ScrolledWindow::builder()
        .child(&preview)
        .hexpand(true)
        .min_content_height(140)
        .max_content_height(140)
        .build();

    // ── Body ──────────────────────────────────────────────────────────────────
    outer.append(&cat_bar);
    outer.append(&list_scroll);
    outer.append(&gtk4::Separator::new(Orientation::Horizontal));
    outer.append(&preview_scroll);
    outer.append(&gtk4::Separator::new(Orientation::Horizontal));

    // ── Footer ─────────────────────────────────────────────────────────────────
    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.set_margin_top(10);
    footer.set_margin_bottom(12);

    let pick_color_btn = Button::with_label("Pick Color…");
    pick_color_btn.set_tooltip_text(Some("Open colour picker and insert hex value at cursor"));
    footer.append(&pick_color_btn);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    footer.append(&spacer);

    let cancel_btn = Button::with_label("Cancel");
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    footer.append(&cancel_btn);
    footer.append(&insert_btn);

    outer.append(&footer);
    dialog.set_child(Some(&outer));

    // ── Build category buttons ─────────────────────────────────────────────────
    let selected_cat: Rc<RefCell<&'static str>> = Rc::new(RefCell::new("All"));

    populate_list(&listbox, "All");

    for &cat in CATEGORIES {
        let btn = Button::with_label(cat);
        btn.add_css_class("flat");
        if cat == "All" {
            btn.add_css_class("suggested-action");
        }
        let lb = listbox.clone();
        let sc = selected_cat.clone();
        let pv = preview.clone();
        let bar = cat_bar.clone();
        btn.connect_clicked(move |clicked| {
            // Update active style
            let mut child = bar.first_child();
            while let Some(w) = child {
                if let Some(b) = w.downcast_ref::<Button>() {
                    b.remove_css_class("suggested-action");
                    b.add_css_class("flat");
                }
                child = w.next_sibling();
            }
            clicked.add_css_class("suggested-action");

            *sc.borrow_mut() = cat;
            populate_list(&lb, cat);
            pv.buffer().set_text("");
        });
        cat_bar.append(&btn);
    }

    // ── Update preview on selection ────────────────────────────────────────────
    {
        let pv = preview.clone();
        listbox.connect_row_selected(move |_, row| {
            if let Some(r) = row {
                let name = r.widget_name();
                if let Some(idx_str) = name.strip_prefix("snippet-") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if let Some(s) = SNIPPETS.get(idx) {
                            pv.buffer().set_text(s.css);
                        }
                    }
                }
            }
        });
    }

    // Select first row to prime the preview
    listbox.select_row(listbox.row_at_index(0).as_ref());

    // ── Cancel ────────────────────────────────────────────────────────────────
    {
        let d = dialog.clone();
        cancel_btn.connect_clicked(move |_| d.close());
    }

    // ── Insert ────────────────────────────────────────────────────────────────
    {
        let d = dialog.clone();
        let nb = notebook.clone();
        let lb = listbox.clone();
        insert_btn.connect_clicked(move |_| {
            if let Some(row) = lb.selected_row() {
                let name = row.widget_name();
                if let Some(idx_str) = name.strip_prefix("snippet-") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if let Some(s) = SNIPPETS.get(idx) {
                            if let Some(tab) = nb.current_tab() {
                                let buf = tab.buffer();
                                let mut cursor = buf.iter_at_mark(&buf.get_insert());
                                buf.begin_user_action();
                                buf.insert(&mut cursor, "\n");
                                buf.insert(&mut cursor, s.css);
                                buf.end_user_action();
                            }
                        }
                    }
                }
            }
            d.close();
        });
    }

    // ── Pick Color ────────────────────────────────────────────────────────────
    {
        let nb = notebook.clone();
        let win = parent.clone();
        pick_color_btn.connect_clicked(move |_| {
            let cd = gtk4::ColorDialog::builder().with_alpha(false).build();
            let nb2 = nb.clone();
            let win2 = win.clone();
            gtk4::glib::MainContext::default().spawn_local(async move {
                if let Ok(color) = cd.choose_rgba_future(Some(&win2), None).await {
                    let r = (color.red() * 255.0) as u8;
                    let g = (color.green() * 255.0) as u8;
                    let b = (color.blue() * 255.0) as u8;
                    let hex = format!("#{r:02x}{g:02x}{b:02x}");
                    if let Some(tab) = nb2.current_tab() {
                        let buf = tab.buffer();
                        let mut cursor = buf.iter_at_mark(&buf.get_insert());
                        buf.begin_user_action();
                        buf.insert(&mut cursor, &hex);
                        buf.end_user_action();
                    }
                }
            });
        });
    }

    dialog.present();
}

fn populate_list(listbox: &ListBox, category: &str) {
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
    }

    for (i, s) in SNIPPETS.iter().enumerate() {
        if category != "All" && s.category != category {
            continue;
        }

        let row_box = GtkBox::new(Orientation::Vertical, 3);
        row_box.set_margin_top(7);
        row_box.set_margin_bottom(7);
        row_box.set_margin_start(10);
        row_box.set_margin_end(10);

        let top = GtkBox::new(Orientation::Horizontal, 8);

        let name_lbl = Label::new(Some(s.name));
        name_lbl.set_halign(Align::Start);
        name_lbl.set_hexpand(true);
        name_lbl.add_css_class("heading");
        top.append(&name_lbl);

        let cat_lbl = Label::new(Some(s.category));
        cat_lbl.set_halign(Align::End);
        cat_lbl.add_css_class("dim-label");
        top.append(&cat_lbl);

        let desc_lbl = Label::new(Some(s.description));
        desc_lbl.set_halign(Align::Start);
        desc_lbl.add_css_class("dim-label");
        desc_lbl.set_wrap(true);

        row_box.append(&top);
        row_box.append(&desc_lbl);

        let row = ListBoxRow::new();
        row.set_child(Some(&row_box));
        // Store the global SNIPPETS index as the widget name for retrieval
        row.set_widget_name(&format!("snippet-{i}"));
        listbox.append(&row);
    }

    listbox.select_row(listbox.row_at_index(0).as_ref());
}
