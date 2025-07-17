use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::string::ToString;
use std::sync::Arc;
use crossbeam_channel::Sender;
use crate::page::{LoginMessage, Network, NetworkEvent, Route, Update, View};
use eframe::egui;
use eframe::egui::Context;
use once_cell::sync::Lazy;
use uuid::Uuid;
use crate::domain::{ConversationId, UserId};
use crate::protocol::network::{MessageError, MessageEvent, MessageSent, NetworkInterface, StreamMessage, WithGeneration};
use crate::shell::AppMessage;

pub enum LobbyMessage {
    Placeholder,
    ChatSent(u64, String),
    ChatReceived(u64, String),
    Stream(StreamMessage),
    MessageSent(String),
    MessageFailed(String),
}

pub struct LobbyPage {
    message_tx: Sender<AppMessage>,
    map_function: Box<dyn Fn(LobbyMessage) -> AppMessage>,
    new_map_function: Arc<Box<dyn Fn(LobbyMessage) -> AppMessage + Send + Sync>>,
    network: Weak<RefCell<dyn Network>>,
    real_network: Rc<RefCell<dyn NetworkInterface>>,

    chat_generation: Option<u64>,
    chat_history: Vec<String>,
    input: String,

    send_to: ConversationKind,
}

impl LobbyPage {
    pub fn new(
        message_tx: Sender<AppMessage>,
        map_function: Box<dyn Fn(LobbyMessage) -> AppMessage>,
        new_map_function: Arc<Box<dyn Fn(LobbyMessage) -> AppMessage + Send + Sync>>,
        network: Weak<RefCell<dyn Network>>,
        real_network: Rc<RefCell<dyn NetworkInterface>>,
        chat_generation: u64,
    ) -> Self {
        Self {
            message_tx: message_tx.clone(),
            map_function,
            new_map_function,
            network,
            real_network,
            chat_generation: Some(chat_generation),
            chat_history: vec![],
            input: String::new(),
            send_to: TEST_CONVERSATIONS.get(0).unwrap().kind
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
            LobbyMessage::MessageSent(message) => {
                self.chat_history.push(message);
            }
            LobbyMessage::MessageFailed(message) => {
                self.chat_history.push(message);
            }
            LobbyMessage::Stream(message) => {
                let message = match message { StreamMessage::Distribute(message) => message };
                self.chat_history.push(message.content);
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
                if ui.button("Logout").clicked() {
                    self.message_tx.send(AppMessage::ReqNavigate(Route::LoginPage)).unwrap();
                }

                ui.separator();

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
                        if !self.input.is_empty() {
                            let conversation_id = &TEST_CONVERSATIONS.iter().find(|e| {
                                e.kind == self.send_to
                            }).unwrap().conversation_id;

                            let input_message = self.input.clone();
                            let message_tx = self.message_tx.clone();
                            let map_function = self.new_map_function.clone();
                            let map = move |event: WithGeneration<MessageEvent>| {
                                let message = match event.result.result {
                                    Ok(_) => LobbyMessage::MessageSent(input_message),
                                    Err(_) => LobbyMessage::MessageFailed(input_message),
                                };
                                let _ = message_tx.send(map_function(message));
                            };

                            let input_message = self.input.clone();
                            let message_tx = self.message_tx.clone();
                            let map_function = self.new_map_function.clone();
                            let map_err = move |_error| {
                                let message = LobbyMessage::MessageFailed(input_message);
                                let _ = message_tx.send(map_function(message));
                            };

                            let _ = self.real_network.borrow_mut().send_chat_message(
                                conversation_id.clone(),
                                self.input.trim().to_string(),
                                1000,
                                Box::new(map),
                                Box::new(map_err),
                            );

                            // self.network.upgrade().unwrap().borrow_mut().send_chat_message(self.chat_generation.unwrap(), self.input.clone(), 1000, Box::new(|e| {
                            //     match e {
                            //         NetworkEvent::ChatSent(generation, message) => {
                            //             AppMessage::Lobby(LobbyMessage::ChatSent(generation, message))
                            //         }
                            //         NetworkEvent::ChatReceived(generation, message) => {
                            //             AppMessage::Lobby(LobbyMessage::ChatReceived(generation, message))
                            //         }
                            //         _ => { AppMessage::PlaceHolder }
                            //     }
                            // })).unwrap();

                            // self.chat_history.push(self.input.trim().to_owned());
                            self.input.clear();
                        }
                        input.request_focus();
                    }
                });
            });

        egui::Window::new("Debug conversations")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                for conversation_info in TEST_CONVERSATIONS.iter() {
                    ui.radio_value(&mut self.send_to, conversation_info.kind, conversation_info.display_name);
                }
            });
    }
}

#[derive(Debug)]
struct UserInfo {
    pub username: String,
    pub user_id: UserId,
}

static TEST_USERS: Lazy<Vec<UserInfo>> = Lazy::new(|| {
    (0..2)
        .map(|i| {
            let username = format!("testuser{}", i);
            let user_id = UserId(Uuid::new_v5(&Uuid::NAMESPACE_OID, username.as_bytes()));
            UserInfo { username, user_id }
        })
        .collect()
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConversationKind {
    Direct,
    Group,
}

#[derive(Debug)]
struct ConversationInfo {
    pub kind: ConversationKind,
    pub display_name: &'static str,
    pub conversation_id: ConversationId,
}

static TEST_CONVERSATIONS: Lazy<Vec<ConversationInfo>> = Lazy::new(|| {
    vec![
        ConversationInfo {
            kind: ConversationKind::Direct,
            display_name: "Direct: 0 ↔ 1",
            conversation_id: ConversationId(Uuid::new_v5(&Uuid::NAMESPACE_OID, b"test_direct0")),
        },
        ConversationInfo {
            kind: ConversationKind::Group,
            display_name: "Group: 0, 1, 2",
            conversation_id: ConversationId(Uuid::new_v5(&Uuid::NAMESPACE_OID, b"test_group0")),
        },
    ]
});
