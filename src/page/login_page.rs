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
use std::rc::{Rc, Weak};
use std::sync::Arc;
use tracing::{event, trace, warn};
use uuid::Uuid;
use crate::protocol::network::{CaptchaData, CaptchaError, CaptchaEvent, LoginError, LoginEvent, NetworkError, NetworkInterface, TokenInfo, WithGeneration};

pub enum LoginMessage {
    PlaceHolder,
    UsernameChanged(String),
    PasswordChanged(String),
    CaptchaChanged(String),
    CaptchaFetched(u64, Uuid, String),
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
    new_map_function: Arc<Box<dyn Fn(LoginMessage) -> AppMessage + Send + Sync>>,
    network: Weak<RefCell<dyn Network>>,
    real_network: Rc<RefCell<dyn NetworkInterface>>,
    username: String,
    password: String,

    captcha: String,
    captcha_generation: Option<u64>,
    captcha_id: Option<Uuid>,
    captcha_base64: String,
    captcha_texture: Option<TextureHandle>,

    login_generation: Option<u64>,
    login_state: Option<LoginState>,
}

impl LoginPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LoginMessage) -> AppMessage>,
        new_map_function: Arc<Box<dyn Fn(LoginMessage) -> AppMessage + Send + Sync>>,
        network: Weak<RefCell<dyn Network>>,
        real_network: Rc<RefCell<dyn NetworkInterface>>,
    ) -> Self {
        let mut captcha_generation = None;
        // fetch_captcha(&mut captcha_generation, network.clone());
        fetch_real_captcha(message_tx.clone(), new_map_function.clone(), &mut captcha_generation, real_network.clone());

        Self {
            message_tx: message_tx.clone(),
            map_function,
            new_map_function,
            network,
            real_network,
            username: "".to_string(),
            password: "".to_string(),
            captcha: "".to_string(),
            captcha_generation,
            captcha_id: None,
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
            LoginMessage::CaptchaFetched(generation, id, base64_string) => {
                if self.captcha_generation == Some(generation) {
                    self.captcha_id = Some(id);
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
                        // fetch_captcha(&mut self.captcha_generation, self.network.clone());
                        fetch_real_captcha(self.message_tx.clone(), self.new_map_function.clone(), &mut self.captcha_generation, self.real_network.clone());
                    }
                } else if let Some(_) = self.captcha_generation {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label("Loading captcha...");
                    });
                } else {
                    if ui.button("Reload captcha").clicked() {
                        // fetch_captcha(&mut self.captcha_generation, self.network.clone());
                        fetch_real_captcha(self.message_tx.clone(), self.new_map_function.clone(), &mut self.captcha_generation, self.real_network.clone());
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
                        login(self.message_tx.clone(), self.new_map_function.clone(),
                              self.username.clone(), self.password.clone(), self.captcha_id.unwrap().clone(), self.captcha.clone(),
                              &mut self.login_generation, self.real_network.clone());

                        // let map_function = |e| match e {
                        //     NetworkEvent::LoginSucceeded(generation, address, jwt) => {
                        //         AppMessage::Login(LoginMessage::LoginSuccess(
                        //             generation, address, jwt,
                        //         ))
                        //     }
                        //     NetworkEvent::LoginFailed(generation) => {
                        //         AppMessage::Login(LoginMessage::LoginFailed(generation))
                        //     }
                        //     _ => AppMessage::PlaceHolder,
                        // };
                        // self.login_generation = self
                        //     .network
                        //     .upgrade()
                        //     .unwrap()
                        //     .borrow_mut()
                        //     .login(self.username.clone(), self.password.clone(), self.captcha.clone(), 1000, Box::new(map_function))
                        //     .ok();
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
            AppMessage::Login(LoginMessage::CaptchaFetched(generation, Uuid::nil(), captcha))
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

fn fetch_real_captcha(
    message_tx: Sender<AppMessage>,
    map_function: Arc<Box<dyn Fn(LoginMessage) -> AppMessage + Send + Sync>>,
    captcha_generation: &mut Option<u64>,
    network: Rc<RefCell<dyn NetworkInterface>>,
) {
    let message_tx_clone = message_tx.clone();
    let map_function_clone = map_function.clone();
    let map = move |event: WithGeneration<CaptchaEvent>| {
        let generation = event.generation;
        let message = match event.result.result {
            Ok(data) => LoginMessage::CaptchaFetched(generation, data.id, data.image_base64),
            Err(_) => LoginMessage::CaptchaFailed(generation),
        };
        let _ = message_tx_clone.send(map_function_clone(message));
    };

    let map_err = move |error: WithGeneration<NetworkError>| {
        let generation = error.generation;
        let message = LoginMessage::CaptchaFailed(generation);
        let _ = message_tx.send(map_function(message));
    };

    *captcha_generation = network.borrow_mut().fetch_captcha(
        1000,
        Box::new(map),
        Box::new(map_err),
    ).ok();
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

fn login(
    message_tx: Sender<AppMessage>,
    map_function: Arc<Box<dyn Fn(LoginMessage) -> AppMessage + Send + Sync>>,
    username: String,
    password: String,
    captcha_id: Uuid,
    captcha_answer: String,
    login_generation: &mut Option<u64>,
    network: Rc<RefCell<dyn NetworkInterface>>,
) {
    let message_tx_clone = message_tx.clone();
    let map_function_clone = map_function.clone();
    let map = move |event: WithGeneration<LoginEvent>| {
        let generation = event.generation;
        let message = match event.result.result {
            Ok(token) => LoginMessage::LoginSuccess(generation, "".to_string(), token.access_token),
            Err(_) => LoginMessage::LoginFailed(generation),
        };
        let _ = message_tx_clone.send(map_function_clone(message));
    };

    let map_err = move |error: WithGeneration<NetworkError>| {
        let generation = error.generation;
        let message = LoginMessage::LoginFailed(generation);
        let _ = message_tx.send(map_function(message));
    };

    *login_generation = network.borrow_mut().login(
        username,
        password,
        captcha_id,
        captcha_answer,
        1000,
        Box::new(map),
        Box::new(map_err),
    ).ok()
}
