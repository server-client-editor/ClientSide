use crate::page::View;
use eframe::egui;
use eframe::egui::Context;

pub struct LobbyPage {
    chat_history: Vec<String>,
    input: String,
}

impl LobbyPage {
    pub fn new() -> Self {
        Self {
            chat_history: vec![],
            input: String::new(),
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
                            self.chat_history.push(self.input.trim().to_owned());
                            self.input.clear();
                        }
                        input.request_focus();
                    }
                });
            });
    }
}
