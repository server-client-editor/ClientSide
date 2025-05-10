use std::cell::RefCell;
use std::rc::Weak;
use crossbeam_channel::Sender;
use eframe::egui;
use eframe::egui::Context;
use tracing::trace;
use crate::page::{Network, Route, View};
use crate::shell::AppMessage;

#[derive(Debug)]
pub enum SignupMessage {
    Placeholder,
}

pub struct SignupPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(SignupMessage) -> AppMessage>,
}

impl SignupPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(SignupMessage) -> AppMessage>,
        _network: Weak<RefCell<dyn Network>>,
    ) -> Self {
        Self {
            message_tx,
            map_function,
        }
    }
}

impl View for SignupPage {
    fn view(&mut self, ctx: &Context) {
        egui::Window::new("Signup")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Go Login").clicked() {
                        trace!("Go Login on Signup");
                        self.message_tx.send(AppMessage::ReqNavigate(Route::LoginPage)).unwrap();
                    }
                    if ui.button("Submit").clicked() {
                        trace!("Submit on Signup");
                    }
                });
            });
    }
}
