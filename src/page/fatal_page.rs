use eframe::egui;
use eframe::egui::Context;
use crate::page::View;

pub struct FatalPage {
    error_message: String,
}

impl FatalPage {
    pub fn new(error_message: String) -> Self {
        Self { error_message }
    }
}

impl View for FatalPage {
    fn view(&mut self, ctx: &Context) {
        egui::Window::new("Fatal error occurred")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Cause: {}", self.error_message));
                ui.label("Please restart the application.");
            });
    }
}