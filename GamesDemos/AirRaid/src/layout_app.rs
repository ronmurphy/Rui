// In main.rs
let game_grid: Grid = builder.object::<Grid>("game_grid").unwrap();
let css_provider = CssProvider::new();
// Your existing CSS provider code goes here...
gtk4::style_context_add_provider_for_display(
    &gtk4::gdk::Display::default().unwrap(),
    &css_provider,
    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
);