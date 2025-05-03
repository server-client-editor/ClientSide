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

use crate::page::{Network, NetworkImpl, Update, View};
use crate::shell::AppMessage;
use base64::Engine;
use crossbeam_channel::Sender;
use eframe::egui;
use eframe::egui::{TextBuffer, TextureHandle, TextureOptions};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::warn;

const IMG_STR: &str = "iVBORw0KGgoAAAANSUhEUgAAAGQAAAAyCAMAAACd646MAAAAP1BMVEUAAAAAOnJjndUIQnpRi8OBu/N8tu4qZJwMRn6Lxf0KRHxSjMRalMwoYppmoNhrpd01b6cgWpI8dq4tZ58WUIhMzA4eAAAAAXRSTlMAQObYZgAAAapJREFUeJzsmM1yhCAMgJN1RmU86Pj+D9vpAiHEwAaxTA+bQ9WK+fKLZuEfysuyaO1kvAyUde2lWBZxBvbxTII4hDKA8YfiqncRnwiic2UKBomXbZp53TmAs8QAKgdPmxo4ooPOU6VEDwIEAKZpskNkB+mepDBhPJ8IDI/kSoXEI0tWZ2VoEIqdT1fC3QWR8QpDGHLfFyquC0QylMfsECRCyoU/R10hxa0hgpiUstAjQsp6vh4yRkuuqIgAgTWmVIOpQLg/NaX6Jd8DGAxSJMOdgq4SJSslBLkTgPCQ940/Fl+diELxWw5Rryz+6ZD5+OEjwK9w3KnjODQIUBEiwJwZ+v5X7SPg98H4GggWlzxJV/M8i2iw3C8FjAPuff642FL8X8lgUVkWlZLMELVISZAk0BhQ84SZq9JZ0V4oBqZOCvVJHY+8waFCsFLCyitW06D3+QMvA4u2MmZvJldMLmxm+/6ZcghFrWZZPIk7QUBUGFs7PlGM67ath2KVEYyvfOWG1Ie1hxiVYY2ke86wMIbM72Mow38l4N8ULWNoE4NB2obdJspPAAAA//9aeATJZ1KZSAAAAABJRU5ErkJggg==";

pub enum LoginMessage {
    PlaceHolder,
    UsernameChanged(String),
    PasswordChanged(String),
    CaptchaChanged(String),
    CaptchaFetched(u64, String),
    CaptchaFailed(u64),
    NavigateTo(String),
}

pub struct LoginPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
    network: Rc<RefCell<dyn Network>>,
    username: String,
    password: String,
    captcha: String,
    captcha_generation: Option<u64>,
    captcha_base64: String,
    captcha_texture: Option<TextureHandle>,
}

impl LoginPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
    ) -> Self {
        Self {
            message_tx,
            map_function,
            network: Rc::new(RefCell::new(NetworkImpl {})),
            username: "".to_string(),
            password: "".to_string(),
            captcha: "".to_string(),
            captcha_generation: None,
            captcha_base64: IMG_STR.to_string(),
            captcha_texture: None,
        }
    }
}

impl Update<LoginMessage> for LoginPage {
    fn update_one(&mut self, message: LoginMessage) {
        match message {
            LoginMessage::UsernameChanged(username) => self.username = username,
            LoginMessage::PasswordChanged(password) => self.password = password,
            LoginMessage::CaptchaFetched(generation, base64_string) => {
                if self.captcha_generation == Some(generation) {
                    self.captcha_base64 = base64_string;
                } else {
                    warn!("Drop one fetched message due to generation mismatch");
                }
            }
            LoginMessage::CaptchaFailed(generation) => {
                if self.captcha_generation == Some(generation) {
                    self.captcha_generation = None;
                } else {
                    warn!("Drop one failed message due to generation mismatch");
                }
            }
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

                ui.label("Password:");
                if ui.text_edit_singleline(&mut self.password).changed() {
                    let map_function = self.map_function.as_ref();
                    self.message_tx
                        .send(map_function(LoginMessage::PasswordChanged(
                            self.password.clone(),
                        )))
                        .unwrap_or_default();
                }

                ui.label("Captcha:");
                if ui.text_edit_singleline(&mut self.captcha).changed() {
                    let map_function = self.map_function.as_ref();
                    self.message_tx.send(map_function(LoginMessage::CaptchaChanged("captcha".to_string()))).unwrap_or_default();
                }
                if !self.captcha_base64.is_empty() {
                    let base64_string = self.captcha_base64.take();
                    self.captcha_texture = load_base64_texture(ctx, &*base64_string, "captcha");
                }

                if let Some(texture) = self.captcha_texture.as_ref() {
                    let image_button = egui::ImageButton::new(texture);
                    if ui.add(image_button).clicked() {
                        self.network
                            .borrow_mut()
                            .fetch_captcha(
                                1000,
                                Box::new(|e| {
                                    AppMessage::Login(LoginMessage::CaptchaFetched(1, "".into()))
                                }),
                            )
                            .unwrap_or_default();
                    }
                } else if let Some(captcha_generation) = self.captcha_generation {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label("Loading captcha...");
                    });
                } else {
                    if ui.button("Reload captcha").clicked() {}
                }
                
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Sign up").clicked() {
                        let map_function = self.map_function.as_ref();
                        self.message_tx
                            .send(map_function(LoginMessage::NavigateTo(
                                "Sign up".to_string(),
                            )))
                            .unwrap_or_default();
                    }

                    if ui.button("Submit").clicked() {
                        let map_function = self.map_function.as_ref();
                        self.message_tx
                            .send(map_function(LoginMessage::NavigateTo("Submit".to_string())))
                            .unwrap_or_default();
                    }
                });
            });
    }
}

fn load_base64_texture(ctx: &egui::Context, encoded: &str, name: &str) -> Option<TextureHandle> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let image_data = image::load_from_memory(&decoded).ok()?;
    let size = [image_data.width() as _, image_data.height() as _];
    let rgba = image_data.to_rgba8();
    let pixels = rgba.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    Some(ctx.load_texture(name, color_image, TextureOptions::default()))
}
