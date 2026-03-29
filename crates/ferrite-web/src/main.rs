fn main() {
    console_log::init_with_level(log::Level::Info).ok();
    dioxus::launch(ferrite_web::app::App);
}
