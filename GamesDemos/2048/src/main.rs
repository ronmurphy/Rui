use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {
    let app = Application::builder()
        .application_id("com.example.game2048")
        .build();

    app.connect_activate(|app| {
        let ui = include_str!("../layout.ui");
        // Strip Rui design-time metadata (rui-*) before passing to GTK Builder.
        let ui_clean: String = ui
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("<property name=\"rui-")
                    && !t.starts_with("<style-name>")
                    && !t.starts_with("<child><layout")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let builder = gtk4::Builder::from_string(&ui_clean);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("2048")
            .default_width(400)
            .default_height(300)
            .build();

        if let Some(root) = builder.object::<gtk4::Widget>("main_box") {
            window.set_child(Some(&root));
        }

        layout_app::connect_handlers(&builder, &window);

        window.present();
    });

    app.run();
}
