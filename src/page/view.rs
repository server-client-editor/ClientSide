use eframe::egui;

pub trait View {
    fn view(&mut self, ctx: &egui::Context);
}