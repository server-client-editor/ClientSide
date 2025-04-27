use eframe::egui;

pub struct App;

impl App {
    pub fn new() -> Self {
        Self {}
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {}
}
