use std::cell::RefCell;
use std::rc::Weak;
use crossbeam_channel::Sender;
use crate::page::{LoginMessage, Network, NetworkEvent, Update, View};
use eframe::egui;
use eframe::egui::Context;
use crate::shell::AppMessage;

pub enum LobbyMessage {
    Placeholder,
    ChatSent(u64, String),
    ChatReceived(u64, String),
}

pub struct LobbyPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(LobbyMessage) -> AppMessage>,
    network: Weak<RefCell<dyn Network>>,

    chat_generation: Option<u64>,
    chat_history: Vec<String>,
    input: String,
}

impl LobbyPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LobbyMessage) -> AppMessage>,
        network: Weak<RefCell<dyn Network>>,
        chat_generation: u64,
    ) -> Self {
        Self {
            message_tx: message_tx.clone(),
            map_function,
            network,
            chat_generation: Some(chat_generation),
            chat_history: vec![],
            input: String::new(),
        }
    }
}

impl Update<LobbyMessage> for LobbyPage {
    fn update_one(&mut self, message: LobbyMessage) {
        match message {
            LobbyMessage::ChatSent(generation, message) => {
                if Some(generation) == self.chat_generation {
                    self.chat_history.push(message);
                }
            }
            LobbyMessage::ChatReceived(generation, message) => {
                if Some(generation) == self.chat_generation {
                    self.chat_history.push(message);
                }
            }
            _ => {}
        }
    }
}

impl View for LobbyPage {
    fn view(&mut self, ctx: &Context) {
        egui::Window::new("Lobby")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .max_height(50.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for message in &self.chat_history {
                            ui.label(message);
                        }
                    });

                ui.separator();

                ui.horizontal(|ui| {
                    let input = ui.text_edit_singleline(&mut self.input);
                    if ui.button("Send").clicked()
                        || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        if !self.input.trim().is_empty() {
                            self.network.upgrade().unwrap().borrow_mut().send_chat_message(self.chat_generation.unwrap(), self.input.clone(), 1000, Box::new(|e| {
                                match e {
                                    NetworkEvent::ChatSent(generation, message) => {
                                        AppMessage::Lobby(LobbyMessage::ChatSent(generation, message))
                                    }
                                    NetworkEvent::ChatReceived(generation, message) => {
                                        AppMessage::Lobby(LobbyMessage::ChatReceived(generation, message))
                                    }
                                    _ => { AppMessage::PlaceHolder }
                                }
                            })).unwrap();

                            // self.chat_history.push(self.input.trim().to_owned());
                            self.input.clear();
                        }
                        input.request_focus();
                    }
                });
            });
    }
}
