use std::time::Instant;
use eframe::egui;
use eframe::egui::Context;
use crate::page::View;

pub struct ShutdownPage {
    deadline: Instant
}

impl ShutdownPage {
    pub fn new(deadline: Instant) -> ShutdownPage {
        ShutdownPage { deadline }
    }
    pub fn get_deadline(&self) -> Instant {
        self.deadline
    }
}

impl View for ShutdownPage {
    fn view(&mut self, ctx: &Context) {
        let now = Instant::now();
        egui::Window::new("Application is exiting")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Cleaning up... The application will close in {} seconds.",
                    (self.deadline - now).as_secs_f32().ceil()
                ));
            });
    }
}