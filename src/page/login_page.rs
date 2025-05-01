//! This module defines the `LoginPage`, which currently depends on `AppMessage`.
//! We accept this dependency to avoid premature abstraction over a `MessageType`.
//!
//! # Design Note
//!
//! If the logic grows or needs generalization, consider using a message converter pattern:
//!
//! ```rust
//! enum Outer {
//!     One(Inner),
//!     Two,
//! }
//!
//! enum Inner {
//!     Three,
//!     Four,
//! }
//!
//! struct Converter<MessageType> {
//!     pub map_function: Box<dyn Fn(Inner) -> MessageType>,
//! }
//!
//! impl<MessageType> Converter<MessageType> {
//!     fn new(map_function: Box<dyn Fn(Inner) -> MessageType>) -> Converter<MessageType> {
//!         Converter { map_function }
//!     }
//!
//!     fn map(&self, smaller: Inner) -> MessageType {
//!         (self.map_function)(smaller)
//!     }
//! }
//!
//! fn main() {
//!     let mut v: Vec<Outer> = Vec::new();
//!     v.push(Outer::One(Inner::Three));
//!     let c = Converter::new(Box::new(|msg| Outer::One(msg)));
//!     v.push(c.map(Inner::Four));
//! }
//! ```

use crate::page::{Update, View};
use crate::shell::AppMessage;
use crossbeam_channel::Sender;
use eframe::egui;
use eframe::egui::TextureHandle;

pub enum LoginMessage {
    PlaceHolder,
    UsernameChanged(String),
}

pub struct LoginPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,

    username: String,
    // password: String,
    // captcha_generation: Option<u64>,
    // captcha_texture: Option<TextureHandle>,
}

impl LoginPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
    ) -> Self {
        Self {
            message_tx,
            map_function,
            username: "".to_string(),
        }
    }
}

impl Update<LoginMessage> for LoginPage {
    fn update_one(&mut self, message: LoginMessage) {
        match message {
            LoginMessage::UsernameChanged(username) => self.username = username,
            _ => {}
        }
    }
}

impl View for LoginPage {
    fn view(&mut self, ctx: &egui::Context) {
        egui::Window::new("Log in")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Username:");
                if ui.text_edit_singleline(&mut self.username).changed() {
                    let map_function = self.map_function.as_ref();
                    self.message_tx
                        .send(map_function(LoginMessage::UsernameChanged(
                            self.username.clone(),
                        )))
                        .unwrap_or_default();
                }
            });
    }
}
