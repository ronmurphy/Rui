use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {
    let app = Application::builder()
        .application_id("com.example.solitare")
        .build();

    app.connect_activate(|app| {
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
            .title("solitare")
            .default_width(800)
            .default_height(600)
            .build();

        if let Some(root) = builder.object::<gtk4::Widget>("main_grid") {
            window.set_child(Some(&root));
        }

        window.present();
    });

    app.run();
}
