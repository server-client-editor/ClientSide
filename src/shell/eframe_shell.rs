//! This module intentionally separates application lifecycle logic from page view rendering.
//!
//! Ideally, the app's exit should be controlled by `update()`, treating the shell
//! (the host runtime) as just another component. However, the current design still
//! depends on the shell to communicate with the OS.
//!
//! While it’s possible to unify shutdown into a `Page` (e.g., a `PoisonPage`),
//! this increases coupling between the UI structure and shell logic, and
//! sacrifices the clarity of centralized lifecycle control.
//!
//! ## Centralized Shutdown Version
//! ```ignore
//! impl eframe::App for App {
//!     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//!         if ctx.input(|i| i.viewport().close_requested()) {
//!             match self.lifecycle {
//!                 Lifecycle::Running => {
//!                     debug!("Closing app");
//!                     ctx.send_viewport_cmd(ViewportCommand::CancelClose);
//!                 }
//!                 Lifecycle::PendingQuit => warn!("Force closed"),
//!                 Lifecycle::QuittingShell => debug!("Graceful shutdown"),
//!             }
//!         }
//!
//!         // Other logic...
//!
//!         if matches!(self.lifecycle, Lifecycle::QuittingShell) {
//!             ctx.send_viewport_cmd(ViewportCommand::Close);
//!         } else {
//!             self.view(ctx);
//!         }
//!     }
//! }
//! ```
//!
//! ## `PoisonPage` Shutdown Version
//! ```ignore
//! pub struct PoisonPage;
//!
//! impl View for PoisonPage {
//!     fn view(&mut self, ctx: &egui::Context) {
//!         ctx.send_viewport_cmd(ViewportCommand::Close);
//!     }
//! }
//!
//! impl eframe::App for App {
//!     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//!         if ctx.input(|i| i.viewport().close_requested()) {
//!             match self.current_page {
//!                 Page::Quit(_) => debug!("Graceful shutdown"),
//!                 Page::Fatal(_) | Page::Shutdown(_) => warn!("Force closed"),
//!                 _ => {
//!                     debug!("Closing app");
//!                     external_messages.push(AppMessage::Exiting);
//!                     ctx.send_viewport_cmd(ViewportCommand::CancelClose);
//!                 }
//!             }
//!         }
//!
//!         // Other logic...
//!
//!         self.view(ctx);
//!     }
//! }
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use crate::page::{Network, FakeNetwork, Update, View, Route, LoginPage, SignupPage, NetworkEvent, LoginMessage, LobbyMessage};
use crate::*;
use anyhow::{anyhow, Result};
use eframe::egui;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, trace, warn};
use crate::protocol::network::{ChatConnError, ChatMetaData, NetworkImpl, NetworkInterface, SessionEvent, StreamMessage, WithGeneration};

const IDLE_POLLING_INTERVAL: Duration = Duration::from_millis(100);
const FAST_POLLING_INTERVAL: Duration = Duration::from_millis(16);
const EXITING_DEADLINE: Duration = Duration::from_secs(5);

pub enum Lifecycle {
    PendingQuit,
    QuitingShell,
    Running,
}

pub enum Page {
    Fatal(page::FatalPage),
    Lobby(page::LobbyPage),
    Login(page::LoginPage),
    Shutdown(page::ShutdownPage),
    Signup(page::SignupPage),
}

pub struct App {
    lifecycle: Lifecycle,
    network: Rc<RefCell<dyn Network>>,
    real_network: Rc<RefCell<dyn NetworkInterface>>,
    chat_generation: Option<u64>,
    stream_buffer: Vec<StreamMessage>,
    current_page: Page,
    message_tx: crossbeam_channel::Sender<AppMessage>,
    message_rx: crossbeam_channel::Receiver<AppMessage>,
    polling_interval: Duration,
}

impl App {
    pub fn new() -> App {
        let (message_tx, message_rx) = crossbeam_channel::unbounded();
        let network: Rc<RefCell<dyn Network>> = Rc::new(RefCell::new(FakeNetwork::new(message_tx.clone())));
        let real_network = Rc::new(RefCell::new(NetworkImpl::try_new().unwrap()));
        App {
            lifecycle: Lifecycle::Running,
            network: network.clone(),
            real_network: real_network.clone(),
            chat_generation: None,
            stream_buffer: Vec::new(),
            current_page: Page::Login(page::LoginPage::new(
                message_tx.clone(),
                Box::new(|m| AppMessage::Login(m)),
                Arc::new(Box::new(|m| AppMessage::Login(m))),
                Rc::downgrade(&network),
                real_network,
            )),
            message_tx,
            message_rx,
            polling_interval: IDLE_POLLING_INTERVAL,
        }
    }
    pub fn shutdown(&mut self) -> Result<()> {
        let deadline = Instant::now() + EXITING_DEADLINE;
        self.lifecycle = Lifecycle::PendingQuit;
        self.current_page = Page::Shutdown(page::ShutdownPage::new(deadline));

        self.polling_interval = FAST_POLLING_INTERVAL;

        Ok(())
    }
    pub fn polling_interval(&self) -> Duration {
        self.polling_interval
    }
}

pub enum AppMessage {
    Quit,
    Exiting,
    PlaceHolder,

    Lobby(page::LobbyMessage),
    Login(page::LoginMessage),
    Signup(page::SignupMessage),

    ReqNavigate(Route),

    Stream(StreamMessage),
}

impl App {
    pub fn poll_internal_events(&mut self) -> Vec<AppMessage> {
        let mut messages = Vec::new();
        let now = Instant::now();

        match &self.current_page {
            Page::Shutdown(inner) => {
                if now >= inner.get_deadline() {
                    messages.push(AppMessage::Quit);
                }
            }
            _ => {}
        }

        messages
    }

    pub fn receive_messages(&mut self, messages: &mut Vec<AppMessage>) {
        for message in messages.drain(..) {
            self.message_tx.send(message).unwrap();
        }
    }

    pub fn update(&mut self) {
        let rx = self.message_rx.clone();
        for message in rx.try_iter() {
            self.update_one(message).unwrap();
        }
    }

    fn update_one(&mut self, message: AppMessage) -> Result<()> {
        match message {
            AppMessage::PlaceHolder => {}
            AppMessage::Exiting => {
                self.shutdown().unwrap();
            }
            AppMessage::Quit => {
                self.lifecycle = Lifecycle::QuitingShell;
            }
            AppMessage::Lobby(message) => match &mut self.current_page {
                Page::Lobby(inner) => {
                    inner.update_one(message);
                }
                _ => {}
            }
            AppMessage::Login(message) => match &mut self.current_page {
                Page::Login(inner) => {
                    inner.update_one(message);
                }
                _ => {}
            },
            AppMessage::Signup(message) => match &mut self.current_page {
                Page::Signup(inner) => {

                }
                _ => {}
            }
            AppMessage::ReqNavigate(route) => {
                debug!("Navigating to {:?}", route);
                match route {
                    Route::LoginPage => {
                        let login_page = LoginPage::new(
                            self.message_tx.clone(),
                            Box::new(|m| AppMessage::Login(m)),
                            Arc::new(Box::new(|m| AppMessage::Login(m))),
                            Rc::downgrade(&self.network),
                            self.real_network.clone(),
                        );
                        self.current_page = Page::Login(login_page);
                    }
                    Route::SignupPage => {
                        let signup_page = SignupPage::new(
                            self.message_tx.clone(),
                            Box::new(|m| AppMessage::Signup(m)),
                            Rc::downgrade(&self.network),
                        );
                        self.current_page = Page::Signup(signup_page);
                    }
                    Route::LobbyPage(address, jwt) => {
                        let message_tx = self.message_tx.clone();
                        let map = move |event: WithGeneration<SessionEvent>| {
                            let message = match event.result.result {
                                Ok(_) => AppMessage::ReqNavigate(Route::ChatConnSuccess),
                                Err(_) => AppMessage::ReqNavigate(Route::ChatConnFailure),
                            };
                            let _ = message_tx.send(message);
                        };

                        let message_tx = self.message_tx.clone();
                        let map_err = move |_error| {
                            let _ = message_tx.send(AppMessage::ReqNavigate(Route::ChatConnFailure));
                        };

                        let message_tx = self.message_tx.clone();
                        self.chat_generation = self.real_network.borrow_mut().connect_chat(
                            address,
                            jwt,
                            Box::new(move |message| {
                                let _ = message_tx.send(AppMessage::Stream(message));
                            }),
                            1000,
                            Box::new(map),
                            Box::new(map_err),
                        ).ok();

                        // self.chat_generation = self.network.borrow_mut().connect_chat(
                        //     address,
                        //     jwt,
                        //     1000,
                        //     Box::new(|e| {
                        //         match e {
                        //             NetworkEvent::ChatConnSucceeded(generation) => {
                        //                 AppMessage::ReqNavigate(Route::ChatConnSuccess)
                        //             }
                        //             NetworkEvent::ChatConnFailed(generation) => {
                        //                 AppMessage::ReqNavigate(Route::ChatConnFailure)
                        //             }
                        //             _ => {AppMessage::PlaceHolder}
                        //         }
                        //     }),
                        // ).ok();
                    }
                    Route::ChatConnSuccess => {
                        let lobby_page = page::LobbyPage::new(
                            self.message_tx.clone(),
                            Box::new(|m| AppMessage::Lobby(m)),
                            Arc::new(Box::new(|m| AppMessage::Lobby(m))),
                            Rc::downgrade(&self.network),
                            self.real_network.clone(),
                            0u64,
                        );
                        self.current_page = Page::Lobby(lobby_page);
                    }
                    Route::ChatConnFailure => {
                        let _ = self.message_tx.send(AppMessage::Login(LoginMessage::ChatFailed));
                    }
                    _ => {
                        warn!("Not implemented yet! {:?}", route);
                    }
                }
            }
            AppMessage::Stream(message) => {
                match &mut self.current_page {
                    Page::Lobby(inner) => {
                        for m in self.stream_buffer.drain(..) {
                            inner.update_one(LobbyMessage::Stream(m));
                        }
                        inner.update_one(LobbyMessage::Stream(message));
                    }
                    _ => {
                        if self.stream_buffer.len() < 1024 {
                            self.stream_buffer.push(message);
                        } else {
                            error!("Drop stream message because buffer is full");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// View block
impl App {
    pub fn view(&mut self, ctx: &egui::Context) {
        match &mut self.current_page {
            Page::Fatal(inner) => inner.view(ctx),
            Page::Lobby(inner) => inner.view(ctx),
            Page::Login(inner) => inner.view(ctx),
            Page::Shutdown(inner) => inner.view(ctx),
            Page::Signup(inner) => inner.view(ctx),
        }
    }
}

// Test block
impl App {
    pub fn new_fatal() -> App {
        let (message_tx, message_rx) = crossbeam_channel::unbounded();
        App {
            lifecycle: Lifecycle::Running,
            network: Rc::new(RefCell::new(FakeNetwork::new(message_tx.clone()))),
            real_network: Rc::new(RefCell::new(NetworkImpl::try_new().unwrap())),
            chat_generation: None,
            stream_buffer: Vec::new(),
            current_page: Page::Fatal(page::FatalPage::new("fatal error".into())),
            message_tx,
            message_rx,
            polling_interval: IDLE_POLLING_INTERVAL,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let start_time = Instant::now();
        let mut external_messages = Vec::<AppMessage>::new();

        // Get input
        if ctx.input(|i| i.viewport().close_requested()) {
            match self.lifecycle {
                Lifecycle::Running => {
                    debug!("Closing app");
                    external_messages.push(AppMessage::Exiting);
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                Lifecycle::PendingQuit => warn!("Force closed"),
                Lifecycle::QuitingShell => debug!("Graceful shutdown"),
            }
        }

        // Pass information to app::receive_events (this populates the message bus)
        self.receive_messages(&mut external_messages);

        // Gather application information
        let mut internal_messages = self.poll_internal_events();
        self.receive_messages(&mut internal_messages);

        // Loop: update application state with app::update(message)
        self.update();

        // Render UI with app::view
        if matches!(self.lifecycle, Lifecycle::QuitingShell) {
            ctx.send_viewport_cmd(egui::viewport::ViewportCommand::Close);
        } else {
            self.view(ctx);
        }

        let elapsed = start_time.elapsed();
        if elapsed >= self.polling_interval() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(self.polling_interval() - elapsed);
        }
    }
}
