use eframe::egui::{self, RichText};

use crate::ui::theme::Theme;
use crate::wallet::Network;

/// Captured send intent so the parent can perform the actual network call.
#[derive(Debug, Clone)]
pub struct SendRequest {
    pub from_address: String,
    pub to_address: String,
    pub amount: f64,
    pub network: Network,
}

/// Modal dialog state for sending crypto.
pub struct SendDialogState {
    pub open: bool,
    pub from_address: String,
    pub to_address: String,
    pub amount: f64,
    pub network: Network,
    pub status: Option<String>,
    pending: Option<SendRequest>,
}

impl SendDialogState {
    pub fn new() -> Self {
        SendDialogState {
            open: false,
            from_address: String::new(),
            to_address: String::new(),
            amount: 0.0,
            network: Network::Bitcoin,
            status: None,
            pending: None,
        }
    }

    pub fn open(&mut self, from: &str, network: Network) {
        self.open = true;
        self.from_address = from.to_string();
        self.network = network;
        self.status = None;
        self.pending = None;
    }

    pub fn attempt_send(&mut self) -> String {
        let trimmed = self.to_address.trim();
        let message = if trimmed.is_empty() || self.amount <= 0.0 {
            "Enter a recipient and positive amount first".to_string()
        } else {
            let request = SendRequest {
                from_address: self.from_address.clone(),
                to_address: trimmed.to_string(),
                amount: self.amount,
                network: self.network,
            };
            self.pending = Some(request);
            format!(
                "Prepared {:.4} {} from {} → {}",
                self.amount,
                self.network.symbol(),
                self.from_address,
                trimmed
            )
        };
        self.status = Some(message.clone());
        message
    }

    pub fn take_pending_request(&mut self) -> Option<SendRequest> {
        self.pending.take()
    }

    pub fn ui(&mut self, ctx: &egui::Context, theme: &Theme) {
        if !self.open {
            return;
        }

        let mut open_state = self.open;
        egui::Window::new("Send Crypto")
            .open(&mut open_state)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Network: {} | From: {}",
                    self.network.name(),
                    self.from_address
                ));

                ui.horizontal(|ui| {
                    ui.label("To:");
                    ui.text_edit_singleline(&mut self.to_address);
                });

                ui.horizontal(|ui| {
                    ui.label("Amount");
                    ui.add(egui::DragValue::new(&mut self.amount).speed(0.1));
                });

                if ui
                    .button(RichText::new("Send").color(theme.green).strong())
                    .clicked()
                {
                    self.attempt_send();
                }

                if let Some(status) = &self.status {
                    ui.separator();
                    ui.label(RichText::new(status).color(theme.blue));
                }
            });
        self.open = open_state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_send_sets_status() {
        let mut dialog = SendDialogState::new();
        dialog.open("bc1qexample", Network::Bitcoin);
        dialog.to_address = "dest".to_string();
        dialog.amount = 1.5;
        let msg = dialog.attempt_send();
        assert!(msg.contains("Prepared 1.5000 BTC"));
        assert!(dialog.status.is_some());
        assert!(dialog.take_pending_request().is_some());
    }

    #[test]
    fn requires_positive_amount() {
        let mut dialog = SendDialogState::new();
        dialog.open("bc1qexample", Network::Bitcoin);
        dialog.attempt_send();
        assert!(dialog.take_pending_request().is_none());
    }
}
