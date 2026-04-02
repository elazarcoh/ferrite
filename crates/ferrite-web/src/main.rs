fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    dioxus::launch(ferrite_web::app::App);
}
