pub struct WebApp;

impl WebApp {
    pub fn new() -> Self {
        WebApp
    }
}

impl eframe::App for WebApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
}
