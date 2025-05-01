use eframe::egui;
use crate::page::View;

pub struct LoginPage;

impl LoginPage {
    pub fn new() -> LoginPage {
        LoginPage
    }
}

impl View for LoginPage {
    fn view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Login page");
        });
    }
}