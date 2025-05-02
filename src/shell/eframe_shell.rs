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
use crate::page::{Network, NetworkImpl, Update, View};
use crate::*;
use anyhow::{anyhow, Result};
use eframe::egui;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, trace, warn};

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
    Login(page::LoginPage),
    Shutdown(page::ShutdownPage),
}

pub struct App {
    lifecycle: Lifecycle,
    network: Rc<RefCell<dyn Network>>,
    current_page: Page,
    message_tx: crossbeam_channel::Sender<AppMessage>,
    message_rx: crossbeam_channel::Receiver<AppMessage>,
    polling_interval: Duration,
}

impl App {
    pub fn new() -> App {
        let (message_tx, message_rx) = crossbeam_channel::unbounded();
        App {
            lifecycle: Lifecycle::Running,
            network: Rc::new(RefCell::new(NetworkImpl {})),
            current_page: Page::Login(page::LoginPage::new(
                message_tx.clone(),
                Box::new(|m| AppMessage::Login(m)),
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

    Login(page::LoginMessage),
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
            AppMessage::Login(message) => match &mut self.current_page {
                Page::Login(inner) => {
                    inner.update_one(message);
                }
                _ => {}
            },
        }
        Ok(())
    }
}

// View block
impl App {
    pub fn view(&mut self, ctx: &egui::Context) {
        match &mut self.current_page {
            Page::Fatal(inner) => inner.view(ctx),
            Page::Login(inner) => inner.view(ctx),
            Page::Shutdown(inner) => inner.view(ctx),
        }
    }
}

// Test block
impl App {
    pub fn new_fatal() -> App {
        let (message_tx, message_rx) = crossbeam_channel::unbounded();
        App {
            lifecycle: Lifecycle::Running,
            network: Rc::new(RefCell::new(NetworkImpl {})),
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
