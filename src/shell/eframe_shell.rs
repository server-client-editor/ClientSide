use anyhow::{anyhow, Result};
use eframe::egui;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, trace, warn};

const IDLE_POLLING_INTERVAL: Duration = Duration::from_millis(100);
const FAST_POLLING_INTERVAL: Duration = Duration::from_millis(16);
const EXITING_DEADLINE: Duration = Duration::from_secs(5);

pub enum AppState {
    Running,
    Exiting(Instant),
    Fatal(String),
    Quit,
}

pub struct App {
    app_state: AppState,
    message_bus: Vec<AppMessage>,
    polling_interval: Duration,
}

impl App {
    pub fn new() -> App {
        App {
            app_state: AppState::Running,
            message_bus: Vec::new(),
            polling_interval: IDLE_POLLING_INTERVAL,
        }
    }
    pub fn shutdown(&mut self) -> Result<()> {
        self.app_state = AppState::Exiting(Instant::now() + EXITING_DEADLINE);

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

        match self.app_state {
            AppState::Exiting(deadline) => {
                if now >= deadline {
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
                self.app_state = AppState::Quit;
            },
        }
        Ok(())
    }
}

// View block
impl App {
    pub fn view(&mut self, ctx: &egui::Context) {
        match self.app_state {
            AppState::Running => {
                // Delegate to page view
            }
            AppState::Exiting(deadline) => {
                let now = Instant::now();
                egui::Window::new("Application is exiting")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Cleaning up... The application will close in {} seconds.",
                            (deadline - now).as_secs()
                        ));
                    });
            }
            AppState::Fatal(ref f) => {
                egui::Window::new("Fatal error occurred")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Cause: {}", f));
                        ui.label("Please restart the application.");
                    });
            }
            AppState::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

// Test block
impl App {
    pub fn new_fatal() -> App {
        App {
            app_state: AppState::Fatal("fatal error".into()),
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
            match self.app_state {
                AppState::Running => {
                    debug!("Closing app");
                    external_messages.push(AppMessage::Exiting);
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                AppState::Quit => {
                    debug!("Graceful shutdown");
                }
                _ => {
                    warn!("Force closed");
                }
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
        self.view(ctx);

        let elapsed = start_time.elapsed();
        if elapsed >= self.polling_interval() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(self.polling_interval() - elapsed);
        }
    }
}
