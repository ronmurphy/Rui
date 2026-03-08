use sourceview5::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation, Separator};
use crate::tab::EditorTab;

#[derive(Clone)]
pub struct StatusBar {
    pub widget:   GtkBox,
    lang_lbl:     Label,
    pos_lbl:      Label,
    modified_lbl: Label,
}

impl StatusBar {
    pub fn new() -> Self {
        let bar = GtkBox::new(Orientation::Horizontal, 0);
        bar.add_css_class("editor-statusbar");

        let lang_lbl = Label::new(Some("Plain Text"));
        lang_lbl.add_css_class("editor-statusbar-item");

        let sep1 = Separator::new(Orientation::Vertical);
        sep1.set_margin_top(4);
        sep1.set_margin_bottom(4);

        let pos_lbl = Label::new(Some("Ln 1, Col 1"));
        pos_lbl.add_css_class("editor-statusbar-item");

        let sep2 = Separator::new(Orientation::Vertical);
        sep2.set_margin_top(4);
        sep2.set_margin_bottom(4);

        let enc_lbl = Label::new(Some("UTF-8"));
        enc_lbl.add_css_class("editor-statusbar-item");

        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        let modified_lbl = Label::new(None);
        modified_lbl.add_css_class("editor-statusbar-modified");
        modified_lbl.set_margin_end(8);

        bar.append(&lang_lbl);
        bar.append(&sep1);
        bar.append(&pos_lbl);
        bar.append(&sep2);
        bar.append(&enc_lbl);
        bar.append(&spacer);
        bar.append(&modified_lbl);

        Self { widget: bar, lang_lbl, pos_lbl, modified_lbl }
    }

    pub fn connect_tab(&self, tab: &EditorTab) {
        self.lang_lbl.set_text(&tab.language_name());
        self.refresh_pos(tab);
        self.refresh_modified(tab.is_modified());

        let pos_lbl = self.pos_lbl.clone();
        let tab_c = tab.clone();
        tab.buffer().connect_notify_local(Some("cursor-position"), move |_, _| {
            let buf = tab_c.buffer();
            let (l, c) = cursor_pos(&buf);
            pos_lbl.set_text(&format!("Ln {}, Col {}", l, c));
        });

        let mod_lbl = self.modified_lbl.clone();
        tab.buffer().connect_modified_changed(move |buf| {
            mod_lbl.set_text(if buf.is_modified() { "● modified" } else { "" });
        });
    }

    fn refresh_pos(&self, tab: &EditorTab) {
        let (l, c) = cursor_pos(&tab.buffer());
        self.pos_lbl.set_text(&format!("Ln {}, Col {}", l, c));
    }

    fn refresh_modified(&self, is_mod: bool) {
        self.modified_lbl.set_text(if is_mod { "● modified" } else { "" });
    }
}

fn cursor_pos(buf: &sourceview5::Buffer) -> (i32, i32) {
    let mark = buf.get_insert();
    let iter = buf.iter_at_mark(&mark);
    (iter.line() + 1, iter.line_offset() + 1)
}
