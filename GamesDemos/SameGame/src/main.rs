use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {
    let app = Application::builder()
        .application_id("com.example.samegame")
        .build();

    app.connect_activate(layout_app::build_game);
    app.run();
}