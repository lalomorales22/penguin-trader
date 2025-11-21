use eframe::egui::{self, RichText};

use crate::ui::theme::Theme;
use crate::wallet::{generate_wallet, Network};

/// Resulting wallet shown in the dialog after generation/import.
#[derive(Debug, Clone)]
pub struct GeneratedWalletPreview {
    pub network: Network,
    pub address: String,
    pub mnemonic: String,
    pub label: String,
    pub private_key: String,
}

/// Modal dialog for generating/importing wallets.
pub struct GenerateDialogState {
    pub open: bool,
    pub network: Network,
    pub label: String,
    pub latest: Option<GeneratedWalletPreview>,
}

impl GenerateDialogState {
    pub fn new() -> Self {
        GenerateDialogState {
            open: false,
            network: Network::Bitcoin,
            label: String::new(),
            latest: None,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.latest = None;
    }

    pub fn attempt_generate(&mut self) -> Option<GeneratedWalletPreview> {
        if let Ok((address, private_key, mnemonic)) = generate_wallet(self.network) {
            let preview = GeneratedWalletPreview {
                network: self.network,
                address,
                mnemonic,
                private_key,
                label: if self.label.trim().is_empty() {
                    format!("{} wallet", self.network.symbol())
                } else {
                    self.label.trim().to_string()
                },
            };
            self.latest = Some(preview.clone());
            Some(preview)
        } else {
            None
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, theme: &Theme) {
        if !self.open {
            return;
        }

        let mut open_state = self.open;
        egui::Window::new("Generate / Import Wallet")
            .open(&mut open_state)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Choose network");
                egui::ComboBox::from_label("")
                    .selected_text(self.network.name())
                    .show_ui(ui, |ui| {
                        for network in [
                            Network::Bitcoin,
                            Network::Ethereum,
                            Network::Solana,
                            Network::Ripple,
                            Network::Cardano,
                        ] {
                            ui.selectable_value(&mut self.network, network, network.name());
                        }
                    });

                ui.label("Label (optional)");
                ui.text_edit_singleline(&mut self.label);

                if ui
                    .button(RichText::new("Generate Wallet").color(theme.green).strong())
                    .clicked()
                {
                    self.attempt_generate();
                }

                if let Some(wallet) = &self.latest {
                    ui.separator();
                    ui.label(RichText::new("Wallet created!").color(theme.green).strong());
                    ui.label(format!("Network: {}", wallet.network.name()));
                    ui.label(format!("Label: {}", wallet.label));
                    ui.label(RichText::new("Address:").color(theme.blue));
                    ui.monospace(wallet.address.clone());
                    ui.label(RichText::new("Mnemonic (store safely):").color(theme.yellow));
                    ui.monospace(wallet.mnemonic.clone());
                }
            });
        self.open = open_state;
    }

    pub fn take_latest(&mut self) -> Option<GeneratedWalletPreview> {
        self.latest.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_wallet_preview() {
        let mut dialog = GenerateDialogState::new();
        dialog.network = Network::Ethereum;
        let result = dialog.attempt_generate();
        assert!(result.is_some());
        let wallet = result.unwrap();
        assert!(wallet.address.starts_with("0x"));
        assert!(!wallet.private_key.is_empty());
    }
}
