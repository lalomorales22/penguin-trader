use std::time::Instant;

use eframe::egui::{self, RichText};

use crate::ui::generate_dialog::{GenerateDialogState, GeneratedWalletPreview};
use crate::ui::send_dialog::SendDialogState;
use crate::ui::theme::Theme;
use crate::wallet::Network;

#[derive(Debug, Clone)]
pub struct WalletEntry {
    pub label: String,
    pub network: Network,
    pub address: String,
    pub balance: f64,
    pub last_synced: Instant,
}

impl WalletEntry {
    pub fn new(label: &str, network: Network, address: &str, balance: f64) -> Self {
        WalletEntry {
            label: label.to_string(),
            network,
            address: address.to_string(),
            balance,
            last_synced: Instant::now(),
        }
    }
}

/// Wallet management tab with list + send/generate dialogs.
pub struct WalletPanel {
    pub wallets: Vec<WalletEntry>,
    pub selected: Option<usize>,
    pub send_dialog: SendDialogState,
    pub generate_dialog: GenerateDialogState,
    pub status: Option<String>,
    refresh_requested: bool,
}

impl WalletPanel {
    pub fn new() -> Self {
        WalletPanel {
            wallets: Vec::new(),
            selected: None,
            send_dialog: SendDialogState::new(),
            generate_dialog: GenerateDialogState::new(),
            status: None,
            refresh_requested: false,
        }
    }

    pub fn replace_wallets(&mut self, wallets: Vec<WalletEntry>) {
        self.wallets = wallets;
        self.selected = if self.wallets.is_empty() { None } else { Some(0) };
    }

    pub fn refresh_balances<F>(&mut self, mut fetcher: F)
    where
        F: FnMut(&WalletEntry) -> Result<f64, String>,
    {
        let mut updated = 0usize;
        for wallet in &mut self.wallets {
            match fetcher(wallet) {
                Ok(balance) => {
                    wallet.balance = balance;
                    wallet.last_synced = Instant::now();
                    updated += 1;
                }
                Err(err) => {
                    self.status = Some(format!(
                        "Failed to refresh {}: {}",
                        wallet.label, err
                    ));
                }
            }
        }
        if updated > 0 {
            self.status = Some(format!("Refreshed {} wallet balance(s)", updated));
        }
    }

    pub fn add_wallet_from_preview(&mut self, preview: &GeneratedWalletPreview) {
        let entry = WalletEntry::new(&preview.label, preview.network, &preview.address, 0.0);
        if !self.wallets.iter().any(|w| w.address == entry.address) {
            self.wallets.push(entry);
            self.selected = Some(self.wallets.len() - 1);
            self.status = Some("Wallet added to dashboard".to_string());
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("Generate Wallet").color(theme.green).strong())
                .clicked()
            {
                self.generate_dialog.open();
            }

            if ui
                .button(RichText::new("Import Wallet").color(theme.blue).strong())
                .clicked()
            {
                self.generate_dialog.open_import();
            }

            if ui
                .button(RichText::new("Send").strong())
                .clicked()
            {
                if let Some(idx) = self.selected {
                    let wallet = &self.wallets[idx];
                    self.send_dialog.open(&wallet.address, wallet.network);
                } else {
                    self.status = Some("Select a wallet first".to_string());
                }
            }

            if ui
                .button(RichText::new("Refresh Balances").color(theme.yellow).strong())
                .clicked()
            {
                // The actual fetch happens in the parent via the request channel.
                self.refresh_requested = true;
                self.status = Some("Refreshing balances...".to_string());
            }
        });

        ui.separator();
        self.render_wallet_table(ui, theme);

        if let Some(status) = &self.status {
            ui.separator();
            ui.label(RichText::new(status).color(theme.blue));
        }

        self.generate_dialog.ui(ctx, theme);
        self.send_dialog.ui(ctx, theme);
    }

    pub fn take_generated_preview(&mut self) -> Option<GeneratedWalletPreview> {
        self.generate_dialog.take_latest()
    }

    pub fn take_pending_send(&mut self) -> Option<crate::ui::send_dialog::SendRequest> {
        self.send_dialog.take_pending_request()
    }

    pub fn take_refresh_request(&mut self) -> bool {
        if self.refresh_requested {
            self.refresh_requested = false;
            true
        } else {
            false
        }
    }

    fn render_wallet_table(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        egui::Grid::new("wallets_grid")
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                ui.label("Label");
                ui.label("Network");
                ui.label("Address");
                ui.label("Balance");
                ui.label("Last Sync");
                ui.end_row();

                for (idx, wallet) in self.wallets.iter_mut().enumerate() {
                    let selected = self.selected == Some(idx);
                    let label = if selected {
                        RichText::new(&wallet.label).color(theme.green).strong()
                    } else {
                        RichText::new(&wallet.label)
                    };
                    if ui
                        .selectable_label(selected, label)
                        .clicked()
                    {
                        self.selected = Some(idx);
                    }
                    ui.label(wallet.network.symbol());
                    ui.monospace(&wallet.address);
                    ui.label(format!("{:.4} {}", wallet.balance, wallet.network.symbol()));
                    let secs = wallet.last_synced.elapsed().as_secs();
                    ui.label(format!("{}s ago", secs));
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn refresh_updates_sync_time() {
        let mut panel = WalletPanel::new();
        panel.replace_wallets(vec![WalletEntry::new(
            "Test",
            Network::Bitcoin,
            "bc1qtest",
            0.5,
        )]);
        let before = panel.wallets[0].last_synced;
        std::thread::sleep(Duration::from_millis(2));
        panel.refresh_balances(|_| Ok(1.0));
        assert!(panel.wallets[0].last_synced > before);
    }

    #[test]
    fn generated_wallet_is_added() {
        let mut panel = WalletPanel::new();
        let preview = GeneratedWalletPreview {
            network: Network::Bitcoin,
            address: "bc1qnew".to_string(),
            mnemonic: "test mnemonic".to_string(),
            label: "New wallet".to_string(),
            private_key: "key".to_string(),
        };
        panel.add_wallet_from_preview(&preview);
        assert!(panel.wallets.iter().any(|w| w.address == "bc1qnew"));
    }
}
