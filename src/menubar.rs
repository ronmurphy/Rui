use gtk4::prelude::*;
use gtk4::gio::{Menu, MenuItem, SimpleAction};
use gtk4::Application;

pub fn build(app: &Application) -> gtk4::PopoverMenuBar {
    let menubar_model = Menu::new();

    // ── File ─────────────────────────────────────────────────────
    let file_menu = Menu::new();
    file_menu.append(Some("New"),            Some("app.new-tab"));
    file_menu.append(Some("New Project…"),  Some("app.new-project"));
    file_menu.append(Some("Open…"),         Some("app.open"));
    file_menu.append(Some("Open Project…"), Some("app.open-project"));
    file_menu.append(Some("Save"),           Some("app.save"));
    file_menu.append(Some("Save All"),       Some("app.save-all"));
    file_menu.append(Some("Save As…"),      Some("app.save-as"));
    file_menu.append(Some("Close Tab"),      Some("app.close-tab"));

    let file_item = MenuItem::new(Some("File"), None);
    file_item.set_submenu(Some(&file_menu));
    menubar_model.append_item(&file_item);

    // ── Edit ─────────────────────────────────────────────────────
    let edit_menu = Menu::new();
    edit_menu.append(Some("Cut"),        Some("app.cut"));
    edit_menu.append(Some("Copy"),       Some("app.copy"));
    edit_menu.append(Some("Paste"),      Some("app.paste"));
    edit_menu.append(Some("Select All"), Some("app.select-all"));
    edit_menu.append(Some("Find…"),          Some("app.find"));
    edit_menu.append(Some("Find & Replace…"), Some("app.find-replace"));
    edit_menu.append(Some("Go to Line…"),     Some("app.goto-line"));

    let edit_item = MenuItem::new(Some("Edit"), None);
    edit_item.set_submenu(Some(&edit_menu));
    menubar_model.append_item(&edit_item);

    // ── View ─────────────────────────────────────────────────────
    let view_menu = Menu::new();
    // Layouts sub-menu
    let layouts_menu = Menu::new();
    layouts_menu.append(Some("Code View"),     Some("app.layout-code"));
    layouts_menu.append(Some("Designer View"), Some("app.layout-designer"));
    view_menu.append_submenu(Some("Layouts"), &layouts_menu);
    view_menu.append(Some("Toggle Sidebar"),  Some("app.toggle-sidebar"));
    view_menu.append(Some("Toggle Output"),   Some("app.toggle-output"));
    view_menu.append(Some("Toggle Canvas"),   Some("app.toggle-canvas"));
    view_menu.append(Some("Toggle Toolbox"),  Some("app.toggle-toolbox"));
    view_menu.append(Some("Toggle Preview"),  Some("app.toggle-preview"));
    view_menu.append(Some("Toggle Minimap"),  Some("app.toggle-minimap"));
    view_menu.append(Some("Toggle Dark Mode"), Some("app.toggle-dark"));

    let view_item = MenuItem::new(Some("View"), None);
    view_item.set_submenu(Some(&view_menu));
    menubar_model.append_item(&view_item);

    // ── Run ──────────────────────────────────────────────────────
    let run_menu = Menu::new();
    run_menu.append(Some("Run"),          Some("app.run"));
    run_menu.append(Some("Build"),        Some("app.build"));
    run_menu.append(Some("Stop"),         Some("app.stop"));
    run_menu.append(Some("Open in Browser"), Some("app.open-browser"));
    run_menu.append(Some("Generate All Handlers"), Some("app.generate-handlers"));
    run_menu.append(Some("Template Library…"), Some("app.template-library"));

    let run_item = MenuItem::new(Some("Run"), None);
    run_item.set_submenu(Some(&run_menu));
    menubar_model.append_item(&run_item);

    // ── AI ────────────────────────────────────────────────────────
    let ai_menu = Menu::new();
    ai_menu.append(Some("Open AI Chat…"),        Some("app.ai-open"));
    ai_menu.append(Some("Copy File for AI"),     Some("app.ai-copy-file"));
    ai_menu.append(Some("Copy Selection for AI"), Some("app.ai-copy-selection"));
    ai_menu.append(Some("Apply AI Diff…"),       Some("app.ai-apply-diff"));

    let ai_item = MenuItem::new(Some("AI"), None);
    ai_item.set_submenu(Some(&ai_menu));
    menubar_model.append_item(&ai_item);

    // ── Help ─────────────────────────────────────────────────────
    let help_menu = Menu::new();
    help_menu.append(Some("Help / Shortcuts"),  Some("app.help"));
    help_menu.append(Some("About Rui"),  Some("app.about"));

    let help_item = MenuItem::new(Some("Help"), None);
    help_item.set_submenu(Some(&help_menu));
    menubar_model.append_item(&help_item);

    for name in &[
        "new-tab", "new-project", "new-ui", "open", "open-project", "save", "save-all", "save-as", "close-tab",
        "cut", "copy", "paste", "select-all", "find", "find-replace", "goto-line",
        "toggle-sidebar", "toggle-output", "toggle-canvas", "toggle-toolbox", "toggle-preview", "toggle-minimap", "toggle-dark",
        "layout-code", "layout-designer",
        "run", "build", "stop", "open-browser", "generate-handlers", "template-library",
        "ai-open", "ai-copy-file", "ai-copy-selection", "ai-apply-diff",
        "help", "about",
    ] {
        let action = SimpleAction::new(name, None);
        app.add_action(&action);
    }

    gtk4::PopoverMenuBar::from_model(Some(&menubar_model))
}

pub fn connect_action<F>(app: &Application, name: &str, cb: F)
where
    F: Fn() + 'static,
{
    if let Some(action) = app.lookup_action(name) {
        if let Some(sa) = action.downcast_ref::<SimpleAction>() {
            sa.connect_activate(move |_, _| cb());
        }
    }
}
