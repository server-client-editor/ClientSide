use std::time::{Duration, Instant};
use anyhow::{anyhow, Result};
use eframe::egui;
use tracing::{debug, error, info, trace, warn};

const IDLE_POLLING_INTERVAL: Duration = Duration::from_secs(1);
const FAST_POLLING_INTERVAL: Duration = Duration::from_millis(16);
const EXITING_DEADLINE: Duration = Duration::from_secs(5);
const FAREWELL_MESSAGE_DURATION: Duration = Duration::from_secs(2);

pub enum AppState {
    Running,
    Exiting(Instant),
    ReadyToExit(Instant),
    Fatal(String),
}

pub struct App {
    app_state: AppState,
    shutdown_signal: Option<crossbeam_channel::Receiver<()>>,
    polling_interval: Duration,
}

impl App {
    pub fn new() -> App {
        App {
            app_state: AppState::Running,
            shutdown_signal: None,
            polling_interval: IDLE_POLLING_INTERVAL,
        }
    }
    pub fn shutdown(&mut self) -> Result<()> {
        self.app_state = AppState::Exiting(Instant::now() + EXITING_DEADLINE);

        let (tx, rx) = crossbeam_channel::bounded(1);
        self.shutdown_signal = Some(rx);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let _ = tx.send(());
        });

        self.polling_interval = FAST_POLLING_INTERVAL;

        Ok(())
    }
    pub fn ready_to_exit(&mut self) -> bool {
        if self.shutdown_signal.as_ref().unwrap().try_recv().is_ok() {
            self.app_state = AppState::ReadyToExit(Instant::now() + FAREWELL_MESSAGE_DURATION);
            true
        } else {
            false
        }
    }
    pub fn polling_interval(&self) -> Duration {
        self.polling_interval
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
                if !self.ready_to_exit() && now >= deadline {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                egui::Window::new("Application is exiting")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Cleaning up... The application will close in {} seconds.", (deadline - now).as_secs()));
                    });
            }
            AppState::ReadyToExit(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                egui::Window::new("Application is exiting")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("Cleanup finished. The application will shut down now.");
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
        }
    }
}

// Test block
impl App {
    pub fn new_fatal() -> App {
        App {
            app_state: AppState::Fatal("fatal error".into()),
            shutdown_signal: None,
            polling_interval: IDLE_POLLING_INTERVAL,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let start_time = Instant::now();

        // Get input
        if ctx.input(|i| i.viewport().close_requested())
            && matches!(self.app_state, AppState::Running)
        {
            if let Err(e) = self.shutdown() {
                error!("Application shutdown failed: {}", e);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // Gather application information

        // Pass information to app::receive_events (this populates the message bus)

        // Loop: update application state with app::update(message)

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
