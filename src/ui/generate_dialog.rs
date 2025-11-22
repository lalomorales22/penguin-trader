use eframe::egui::{self, RichText};

use crate::ui::theme::Theme;
use crate::wallet::{generate_wallet, import_wallet_from_private_key, Network};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogMode {
    Generate,
    Import,
}

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
    pub mode: DialogMode,
    pub network: Network,
    pub label: String,
    pub private_key_input: String,
    pub import_error: Option<String>,
    pub latest: Option<GeneratedWalletPreview>,
}

impl GenerateDialogState {
    pub fn new() -> Self {
        GenerateDialogState {
            open: false,
            mode: DialogMode::Generate,
            network: Network::Bitcoin,
            label: String::new(),
            private_key_input: String::new(),
            import_error: None,
            latest: None,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.mode = DialogMode::Generate;
        self.latest = None;
        self.private_key_input.clear();
        self.import_error = None;
    }

    pub fn open_import(&mut self) {
        self.open = true;
        self.mode = DialogMode::Import;
        self.latest = None;
        self.private_key_input.clear();
        self.import_error = None;
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

    pub fn attempt_import(&mut self) -> Option<GeneratedWalletPreview> {
        self.import_error = None;

        if self.private_key_input.trim().is_empty() {
            self.import_error = Some("Private key cannot be empty".to_string());
            return None;
        }

        match import_wallet_from_private_key(self.network, self.private_key_input.trim()) {
            Ok(address) => {
                let preview = GeneratedWalletPreview {
                    network: self.network,
                    address,
                    mnemonic: String::new(), // No mnemonic for imports
                    private_key: self.private_key_input.trim().to_string(),
                    label: if self.label.trim().is_empty() {
                        format!("Imported {} wallet", self.network.symbol())
                    } else {
                        self.label.trim().to_string()
                    },
                };
                self.latest = Some(preview.clone());
                Some(preview)
            }
            Err(e) => {
                self.import_error = Some(e);
                None
            }
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
            .min_width(450.0)
            .show(ctx, |ui| {
                // Mode selection tabs
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.mode, DialogMode::Generate, "Generate");
                    ui.selectable_value(&mut self.mode, DialogMode::Import, "Import");
                });

                ui.separator();

                // Network selection (common for both modes)
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
                            Network::Bnb,
                        ] {
                            ui.selectable_value(&mut self.network, network, network.name());
                        }
                    });

                ui.add_space(5.0);

                // Label input (common for both modes)
                ui.label("Label (optional)");
                ui.text_edit_singleline(&mut self.label);

                ui.add_space(5.0);

                // Mode-specific UI
                match self.mode {
                    DialogMode::Generate => {
                        if ui
                            .button(RichText::new("Generate Wallet").color(theme.green).strong())
                            .clicked()
                        {
                            self.attempt_generate();
                        }
                    }
                    DialogMode::Import => {
                        ui.label(RichText::new("Private Key (hex format):").color(theme.yellow));
                        ui.text_edit_singleline(&mut self.private_key_input);

                        ui.add_space(5.0);

                        // Show format hint based on selected network
                        let hint = match self.network {
                            Network::Bitcoin => "64 hex characters (32 bytes)",
                            Network::Ethereum | Network::Bnb => "64 hex characters, with or without 0x prefix",
                            Network::Solana => "64 hex characters (32 bytes)",
                            Network::Ripple => "64 hex characters (32 bytes)",
                            Network::Cardano => "64 hex characters (32 bytes)",
                        };
                        ui.label(RichText::new(format!("Expected: {}", hint)).italics().weak());

                        ui.add_space(5.0);

                        // Show error if any
                        if let Some(error) = &self.import_error {
                            ui.label(RichText::new(format!("Error: {}", error)).color(theme.red));
                        }

                        if ui
                            .button(RichText::new("Import Wallet").color(theme.blue).strong())
                            .clicked()
                        {
                            self.attempt_import();
                        }
                    }
                }

                // Show result (common for both modes)
                if let Some(wallet) = &self.latest {
                    ui.separator();
                    ui.label(RichText::new(
                        if self.mode == DialogMode::Generate {
                            "Wallet generated!"
                        } else {
                            "Wallet imported!"
                        }
                    ).color(theme.green).strong());

                    ui.label(format!("Network: {}", wallet.network.name()));
                    ui.label(format!("Label: {}", wallet.label));
                    ui.label(RichText::new("Address:").color(theme.blue));
                    ui.monospace(wallet.address.clone());

                    // Only show mnemonic for generated wallets
                    if self.mode == DialogMode::Generate && !wallet.mnemonic.is_empty() {
                        ui.label(RichText::new("Mnemonic (store safely):").color(theme.yellow));
                        ui.monospace(wallet.mnemonic.clone());
                    }
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
