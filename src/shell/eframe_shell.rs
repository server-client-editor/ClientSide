use crate::*;
use crate::page::View;
use anyhow::{anyhow, Result};
use eframe::egui;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, trace, warn};

const IDLE_POLLING_INTERVAL: Duration = Duration::from_millis(100);
const FAST_POLLING_INTERVAL: Duration = Duration::from_millis(16);
const EXITING_DEADLINE: Duration = Duration::from_secs(5);

/// Separating application lifecycle from page view is intentional.
/// Ideally, the app's exit should be controlled by `update()`,
/// and the shell (the outer host) should be treated as a component
/// within our app. However, we still rely on the shell to
/// communicate with the operating system.
///
/// Although it's possible to unify quitting into a `Page` via
/// a `PoisonPage`, doing so sacrifices the clarity of centralized
/// shutdown logic and increases the coupling between shell logic
/// and UI structure.
///
/// This comment documents both approaches:
///
/// Centralized shutdown version:
/// ```ignore
/// impl eframe::App for App {
///     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
///         // Handle close requests
///         if ctx.input(|i| i.viewport().close_requested()) {
///             match self.lifecycle {
///                 Lifecycle::Running => {
///                     debug!("Closing app");
///                     ctx.send_viewport_cmd(ViewportCommand::CancelClose);
///                 }
///                 Lifecycle::PendingQuit => warn!("Force closed"),
///                 Lifecycle::QuittingShell => debug!("Graceful shutdown"),
///             }
///         }
///
///         /* Other logic */
///
///         // Render UI
///         if matches!(self.lifecycle, Lifecycle::QuittingShell) {
///             ctx.send_viewport_cmd(ViewportCommand::Close);
///         } else {
///             self.view(ctx);
///         }
///     }
/// }
/// ```
///
/// `PoisonPage` shutdown version:
/// ```ignore
/// pub struct PoisonPage;
///
/// impl View for PoisonPage {
///     fn view(&mut self, ctx: &egui::Context) {
///         ctx.send_viewport_cmd(ViewportCommand::Close);
///     }
/// }
///
/// impl eframe::App for App {
///     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
///         // Handle close requests
///         if ctx.input(|i| i.viewport().close_requested()) {
///             match self.current_page {
///                 Page::Quit(_) => debug!("Graceful shutdown"),
///                 Page::Fatal(_) | Page::Shutdown(_) => warn!("Force closed"),
///                 _ => {
///                     debug!("Closing app");
///                     external_messages.push(AppMessage::Exiting);
///                     ctx.send_viewport_cmd(ViewportCommand::CancelClose);
///                 }
///             }
///         }
///
///         /* Other logic */
///
///         // Render UI
///         self.view(ctx);
///     }
/// }
/// ```
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
    current_page: Page,
    message_bus: Vec<AppMessage>,
    polling_interval: Duration,
}

impl App {
    pub fn new() -> App {
        App {
            lifecycle: Lifecycle::Running,
            current_page: Page::Login(page::LoginPage::new()),
            message_bus: Vec::new(),
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
        self.message_bus.append(messages);
    }

    pub fn update(&mut self) {
        let mut message_bus = std::mem::take(&mut self.message_bus);
        for message in message_bus.drain(..) {
            self.update_one(message).unwrap(); // Is dropping messages a good idea?
        }
    }

    fn update_one(&mut self, message: AppMessage) -> Result<()> {
        match message {
            AppMessage::PlaceHolder => {},
            AppMessage::Exiting => {
                self.shutdown().unwrap();
            },
            AppMessage::Quit => {
                self.lifecycle = Lifecycle::QuitingShell;
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
        App {
            lifecycle: Lifecycle::Running,
            current_page: Page::Fatal(page::FatalPage::new("fatal error".into())),
            message_bus: Vec::new(),
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
