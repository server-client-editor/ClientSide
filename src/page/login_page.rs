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

use crate::page::{FakeNetwork, Network, NetworkEvent, Route, Update, View};
use crate::shell::AppMessage;
use base64::Engine;
use crossbeam_channel::Sender;
use eframe::egui;
use eframe::egui::{TextBuffer, TextureHandle, TextureOptions};
use std::cell::RefCell;
use std::rc::Weak;
use tracing::{trace, warn};

pub enum LoginMessage {
    PlaceHolder,
    UsernameChanged(String),
    PasswordChanged(String),
    CaptchaChanged(String),
    CaptchaFetched(u64, String),
    CaptchaFailed(u64),
    LoginSuccess(u64, String, String),
    LoginFailed(u64),
    ChatFailed,
    NavigateTo(String),
}

pub enum LoginState {
    RequestSent,
    Success(String, String),
    Failure(String),
    ChatFailed,
}

pub struct LoginPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
    network: Weak<RefCell<dyn Network>>,
    username: String,
    password: String,

    captcha: String,
    captcha_generation: Option<u64>,
    captcha_base64: String,
    captcha_texture: Option<TextureHandle>,

    login_generation: Option<u64>,
    login_state: Option<LoginState>,
}

impl LoginPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
        network: Weak<RefCell<dyn Network>>,
    ) -> Self {
        let mut captcha_generation = None;
        fetch_captcha(&mut captcha_generation, network.clone());

        Self {
            message_tx: message_tx.clone(),
            map_function,
            network,
            username: "".to_string(),
            password: "".to_string(),
            captcha: "".to_string(),
            captcha_generation,
            captcha_base64: "".to_string(),
            captcha_texture: None,
            login_generation: None,
            login_state: None,
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
                    self.captcha_texture = None;
                } else {
                    warn!("Drop one failed message due to generation mismatch");
                }
            }
            LoginMessage::LoginSuccess(generation, address, jwt) => {
                if self.login_generation == Some(generation) {
                    self.login_state = Some(LoginState::Success(address.clone(), jwt.clone()));
                    self.message_tx.send(AppMessage::ReqNavigate(Route::LobbyPage(address, jwt))).unwrap();
                }
            }
            LoginMessage::LoginFailed(generation) => {
                if self.login_generation == Some(generation) {
                    self.login_state = Some(LoginState::Failure("Login failed".to_string()));
                } else {
                    warn!("Drop one failed message due to generation mismatch");
                }
            }
            LoginMessage::ChatFailed => {
                self.login_state = Some(LoginState::ChatFailed);
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
                    self.message_tx
                        .send(map_function(LoginMessage::CaptchaChanged(
                            "captcha".to_string(),
                        )))
                        .unwrap_or_default();
                }
                if !self.captcha_base64.is_empty() {
                    let base64_string = self.captcha_base64.take();
                    self.captcha_texture = load_base64_texture(ctx, &*base64_string, "captcha");
                }

                if let Some(texture) = self.captcha_texture.as_ref() {
                    let image_button = egui::ImageButton::new(texture);
                    if ui.add(image_button).clicked() {
                        self.captcha_texture = None;
                        fetch_captcha(&mut self.captcha_generation, self.network.clone());
                    }
                } else if let Some(_) = self.captcha_generation {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label("Loading captcha...");
                    });
                } else {
                    if ui.button("Reload captcha").clicked() {
                        fetch_captcha(&mut self.captcha_generation, self.network.clone());
                    }
                }

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Sign up").clicked() {
                        self.message_tx.send(AppMessage::ReqNavigate(Route::SignupPage)).unwrap();
                        // let map_function = self.map_function.as_ref();
                        // self.message_tx
                        //     .send(map_function(LoginMessage::NavigateTo(
                        //         "Sign up".to_string(),
                        //     )))
                        //     .unwrap_or_default();
                    }

                    let enabled = matches!(
                        self.login_state,
                        None | Some(LoginState::Failure(_)) | Some(LoginState::ChatFailed),
                    );
                    if ui.add_enabled(enabled, egui::Button::new("Submit")).clicked() {
                        self.login_state = Some(LoginState::RequestSent);
                        let map_function = |e| match e {
                            NetworkEvent::LoginSucceeded(generation, address, jwt) => {
                                AppMessage::Login(LoginMessage::LoginSuccess(
                                    generation, address, jwt,
                                ))
                            }
                            NetworkEvent::LoginFailed(generation) => {
                                AppMessage::Login(LoginMessage::LoginFailed(generation))
                            }
                            _ => AppMessage::PlaceHolder,
                        };
                        self.login_generation = self
                            .network
                            .upgrade()
                            .unwrap()
                            .borrow_mut()
                            .login(self.username.clone(), self.password.clone(), self.captcha.clone(), 1000, Box::new(map_function))
                            .ok();
                    }

                    if let Some(ref state) = self.login_state {
                        ui.horizontal(|ui| match state {
                            LoginState::RequestSent => {
                                ui.add(egui::Spinner::new());
                                ui.label("Waiting for authentication...");
                            }
                            LoginState::Success(_, _) => {
                                ui.add(egui::Spinner::new());
                                ui.label("Establishing connection...");
                            }
                            LoginState::Failure(reason) => {
                                ui.label(format!("Login failed: {}", reason));
                            }
                            LoginState::ChatFailed => {
                                ui.label("Failed to connect to chat server. Please retry.");
                            }
                        });
                    }
                });
            });
    }
}

fn fetch_captcha(captcha_generation: &mut Option<u64>, network: Weak<RefCell<dyn Network>>) {
    let map_function = |e: NetworkEvent| match e {
        NetworkEvent::CaptchaFetched(generation, captcha) => {
            AppMessage::Login(LoginMessage::CaptchaFetched(generation, captcha))
        }
        NetworkEvent::CaptchaFailed(generation) => {
            AppMessage::Login(LoginMessage::CaptchaFailed(generation))
        }
        _ => AppMessage::Login(LoginMessage::PlaceHolder),
    };
    *captcha_generation = network
        .upgrade()
        .unwrap()
        .borrow_mut()
        .fetch_captcha(1000, Box::new(map_function))
        .ok();
}

fn load_base64_texture(ctx: &egui::Context, encoded: &str, name: &str) -> Option<TextureHandle> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let image_data = image::load_from_memory(&decoded).ok()?;
    let size = [image_data.width() as _, image_data.height() as _];
    let rgba = image_data.to_rgba8();
    let pixels = rgba.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    Some(ctx.load_texture(name, color_image, TextureOptions::default()))
}
