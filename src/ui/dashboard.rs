use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use eframe::egui::{self, RichText};

use crate::trading::SharedDataStore;
use crate::ui::chart_deck::ChartDeckPanel;
use crate::ui::positions_panel::{PositionRow, PositionsPanel};
use crate::ui::scanner_panel::ScannerPanel;
use crate::ui::theme::Theme;
use crate::ui::wallet_panel::{WalletEntry, WalletPanel};
use crate::ui::{generate_dialog::GeneratedWalletPreview, send_dialog::SendRequest};
use crate::wallet::{
    decrypt, encrypt, estimate_fee, fetch_balance, send_transaction, ChartSnapshot, EncryptedData, Wallet,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Scanner,
    Positions,
    Wallets,
    Charts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMode {
    Paper,
    Live,
}

impl TradingMode {
    pub fn label(&self) -> &'static str {
        match self {
            TradingMode::Paper => "Paper",
            TradingMode::Live => "Live",
        }
    }

    fn toggle(self) -> Self {
        match self {
            TradingMode::Paper => TradingMode::Live,
            TradingMode::Live => TradingMode::Paper,
        }
    }

    pub fn storage_label(&self) -> &'static str {
        match self {
            TradingMode::Paper => "paper",
            TradingMode::Live => "live",
        }
    }
}

/// Main egui application that ties together scanner, positions, and wallets.
pub struct DashboardApp {
    theme: Theme,
    active_tab: Tab,
    pub scanner: ScannerPanel,
    pub positions: PositionsPanel,
    pub wallets: WalletPanel,
    pub chart_deck: ChartDeckPanel,
    last_scan: Instant,
    portfolio_balance: f64,
    mode: TradingMode,
    shared: Option<SharedDataStore>,
    runtime: tokio::runtime::Runtime,
    status: Option<String>,
    master_password: String,
}

impl DashboardApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme = Theme::new();
        theme.apply(&cc.egui_ctx);

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let shared = SharedDataStore::new("wallets.db", "penguin_data.json").ok();

        let mut app = DashboardApp {
            theme,
            active_tab: Tab::Scanner,
            scanner: ScannerPanel::new(),
            positions: PositionsPanel::new(),
            wallets: WalletPanel::new(),
            chart_deck: ChartDeckPanel::new(),
            last_scan: Instant::now(),
            portfolio_balance: 10_000.0,
            mode: TradingMode::Paper,
            shared,
            runtime,
            status: None,
            master_password: String::new(),
        };
        app.apply_mode_env();
        app.sync_from_shared_store();
        app
    }

    fn apply_mode_env(&self) {
        match self.mode {
            TradingMode::Paper => std::env::set_var("PENGUIN_TEST_MODE", "1"),
            TradingMode::Live => std::env::remove_var("PENGUIN_TEST_MODE"),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let heat = self.positions.total_unrealized();
        ui.horizontal(|ui| {
            ui.label(RichText::new("🐧 Penguin Trader").color(self.theme.green).strong());
            ui.separator();
            ui.label(format!("Balance: ${:.0}", self.portfolio_balance));
            ui.label(format!("Unrealized: {:+.0}", heat));
            ui.label(format!(
                "Scanner auto-refresh: {}s",
                self.last_scan.elapsed().as_secs()
            ));
            ui.separator();
            ui.label(format!("Mode: {}", self.mode.label()));
            let toggle_label = match self.mode {
                TradingMode::Paper => "Go Live",
                TradingMode::Live => "Back to Paper",
            };
            if ui
                .button(RichText::new(toggle_label).color(self.theme.yellow).strong())
                .clicked()
            {
                self.toggle_mode();
            }
            ui.label("Master Password:");
            ui.add(
                egui::TextEdit::singleline(&mut self.master_password)
                    .password(true)
                    .desired_width(150.0),
            );
            if ui
                .button(RichText::new("Sync Shared DB").color(self.theme.blue).strong())
                .clicked()
            {
                self.sync_from_shared_store();
            }
            if let Some(status) = &self.status {
                ui.separator();
                ui.label(RichText::new(status).color(self.theme.blue));
            }
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Scanner, "Scanner"),
                (Tab::Positions, "Positions"),
                (Tab::Wallets, "Wallets"),
                (Tab::Charts, "Chart Deck"),
            ] {
                let clicked = ui
                    .selectable_label(self.active_tab == tab, RichText::new(label).strong())
                    .clicked();
                if clicked {
                    self.active_tab = tab;
                }
            }
        });
    }

    fn tick(&mut self) {
        if self.last_scan.elapsed() >= Duration::from_secs(5) {
            self.scanner.refresh();
            self.last_scan = Instant::now();
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = self.mode.toggle();
        self.apply_mode_env();
        self.status = Some(format!("Switched to {} mode", self.mode.label()));
        self.sync_charts_from_db();
    }

    fn sync_from_shared_store(&mut self) {
        self.sync_positions_from_app_data();
        self.sync_wallets_from_db();
        self.sync_charts_from_db();
    }

    fn sync_positions_from_app_data(&mut self) {
        if let Some(store) = &self.shared {
            let data = store.load_app_data_for_mode(self.mode.storage_label());
            self.portfolio_balance = data.portfolio.balance;
            let rows: Vec<PositionRow> = data
                .open_positions
                .iter()
                .map(|pos| PositionRow {
                    symbol: pos.symbol.clone(),
                    action: pos.side.as_action(),
                    entry: pos.entry_price,
                    last: pos.approximate_last(),
                    size: pos.size,
                    leverage: pos.leverage,
                    stop_loss: pos.stop_loss,
                    take_profit: pos.take_profit,
                    unrealized_pnl: pos.unrealized_pnl,
                    unrealized_pct: pos.unrealized_pnl_percent,
                })
                .collect();
            self.positions.set_positions(rows);
            self.status = Some(format!(
                "Loaded shared trades from {}",
                store.app_data_path_for_mode(self.mode.storage_label())
            ));
        }
    }

    fn sync_wallets_from_db(&mut self) {
        if let Some(store) = &self.shared {
            match store.wallet_db.load_all_wallets() {
                Ok(wallets) => {
                    let mut entries = Vec::new();
                    for wallet in wallets {
                        let balance = self
                            .runtime
                            .block_on(fetch_balance(&wallet.address, wallet.network))
                            .map(|b| b.balance)
                            .unwrap_or(0.0);
                        let label = wallet
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{} wallet", wallet.network.symbol()));
                        entries.push(WalletEntry::new(
                            &label,
                            wallet.network,
                            &wallet.address,
                            balance,
                        ));
                    }
                    self.wallets.replace_wallets(entries);
                    self.status = Some(format!(
                        "Synced {} wallet(s) from {}",
                        self.wallets.wallets.len(),
                        store.wallet_path()
                    ));
                }
                Err(e) => {
                    self.status = Some(format!("Wallet sync failed: {}", e));
                }
            }
        }
    }

    fn sync_charts_from_db(&mut self) {
        if let Some(store) = &self.shared {
            match store
                .wallet_db
                .load_chart_snapshots(Some(self.mode.storage_label()))
            {
                Ok(charts) => {
                    let count = charts.len();
                    self.chart_deck.set_charts(charts);
                    self.chart_deck.status = Some(format!(
                        "Loaded {} chart(s) for {} mode",
                        count,
                        self.mode.label()
                    ));
                }
                Err(e) => {
                    self.chart_deck.status = Some(format!("Chart sync failed: {}", e));
                }
            }
        }
    }

    fn persist_generated_wallet(&mut self, preview: GeneratedWalletPreview) {
        let Some(store) = &self.shared else {
            self.wallets.status = Some("Wallet DB unavailable".to_string());
            return;
        };
        if self.master_password.is_empty() {
            self.wallets.status = Some("Enter master password to save wallet".to_string());
            return;
        }
        let encrypted = match encrypt(preview.private_key.as_bytes(), &self.master_password) {
            Ok(enc) => enc,
            Err(e) => {
                self.wallets.status = Some(format!("Encryption failed: {}", e));
                return;
            }
        };
        let wallet = Wallet {
            id: format!("wallet_gui_{}", Self::generate_id()),
            network: preview.network,
            address: preview.address.clone(),
            encrypted_private_key: encrypted.ciphertext.clone(),
            encrypted_mnemonic: Some(encrypted.ciphertext.clone()),
            encryption_salt: encrypted.salt.clone(),
            encryption_nonce: encrypted.nonce.clone(),
            label: Some(preview.label.clone()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        match store.wallet_db.save_wallet(&wallet) {
            Ok(_) => {
                self.wallets.add_wallet_from_preview(&preview);
                self.wallets.status = Some(format!(
                    "Saved {} to {}",
                    preview.label,
                    store.wallet_path()
                ));
                // refresh balances after adding
                self.sync_wallets_from_db();
            }
            Err(e) => {
                self.wallets.status = Some(format!("Failed to save wallet: {}", e));
            }
        }
    }

    fn persist_chart_snapshot(&mut self, mut snapshot: ChartSnapshot) {
        let Some(store) = &self.shared else {
            self.chart_deck.status = Some("Chart storage unavailable".to_string());
            return;
        };
        let now = Utc::now().to_rfc3339();
        snapshot.mode = self.mode.storage_label().to_string();
        snapshot.updated_at = now.clone();
        if snapshot.created_at.is_empty() {
            snapshot.created_at = now;
        }

        match store.wallet_db.save_chart_snapshot(&snapshot) {
            Ok(_) => {
                self.chart_deck.add_or_replace(snapshot.clone());
                self.chart_deck.status = Some(format!(
                    "Saved {} chart snapshot ({})",
                    snapshot.asset, self.mode.label()
                ));
            }
            Err(e) => {
                self.chart_deck.status = Some(format!("Failed to save chart: {}", e));
            }
        }
    }

    fn delete_chart_snapshot(&mut self, id: String) {
        let Some(store) = &self.shared else {
            self.chart_deck.status = Some("Chart storage unavailable".to_string());
            return;
        };
        match store.wallet_db.delete_chart_snapshot(&id) {
            Ok(true) => {
                self.chart_deck.charts.retain(|c| c.id != id);
                self.chart_deck.status = Some("Deleted chart snapshot".to_string());
            }
            Ok(false) => {
                self.chart_deck.status = Some("Chart not found".to_string());
            }
            Err(e) => {
                self.chart_deck.status = Some(format!("Delete failed: {}", e));
            }
        }
    }

    fn process_send_request(&mut self, request: SendRequest) {
        self.apply_mode_env();
        let Some(store) = &self.shared else {
            self.wallets.status = Some("Wallet DB unavailable".to_string());
            return;
        };
        if self.master_password.is_empty() {
            self.wallets.status = Some("Enter master password to send".to_string());
            return;
        }
        let wallet = match store.wallet_db.load_wallet(&request.from_address) {
            Ok(Some(w)) => w,
            Ok(None) => {
                self.wallets.status = Some("Wallet not found in shared DB".to_string());
                return;
            }
            Err(e) => {
                self.wallets.status = Some(format!("Wallet lookup failed: {}", e));
                return;
            }
        };

        let encrypted = EncryptedData {
            ciphertext: wallet.encrypted_private_key.clone(),
            salt: wallet.encryption_salt.clone(),
            nonce: wallet.encryption_nonce.clone(),
        };

        let private_key = match decrypt(&encrypted, &self.master_password) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                self.wallets
                    .status = Some(format!("Unlock failed: {}", e));
                return;
            }
        };

        let fee = match self
            .runtime
            .block_on(estimate_fee(request.network, request.amount))
        {
            Ok(f) => f,
            Err(e) => {
                self.wallets.status = Some(format!("Fee estimate failed: {:?}", e));
                return;
            }
        };

        match self.runtime.block_on(send_transaction(
            request.network,
            &request.from_address,
            &request.to_address,
            request.amount,
            &private_key,
            fee,
        )) {
            Ok(tx) => {
                self.wallets.status = Some(format!(
                    "{} send prepared: {} → {} | fee {:.6} ({})",
                    request.network.symbol(),
                    tx.from_address,
                    tx.to_address,
                    tx.fee,
                    tx.tx_hash
                ));
                // keep balances updated after a send request
                self.sync_wallets_from_db();
            }
            Err(e) => {
                self.wallets.status = Some(format!("Send failed: {}", e));
            }
        }
    }

    fn generate_id() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        format!("{:x}", ts.as_micros())
    }
}

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick();

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            self.header(ui);
            self.tabs(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            Tab::Scanner => self.scanner.show(ui, &self.theme),
            Tab::Positions => self.positions.show(ui, &self.theme),
            Tab::Wallets => self.wallets.show(ctx, ui, &self.theme),
            Tab::Charts => self
                .chart_deck
                .show(ui, &self.theme, self.mode.storage_label()),
        });

        if self.wallets.take_refresh_request() {
            self.sync_wallets_from_db();
        }

        if let Some(preview) = self.wallets.take_generated_preview() {
            self.persist_generated_wallet(preview);
        }

        if let Some(send_request) = self.wallets.take_pending_send() {
            self.process_send_request(send_request);
        }

        if let Some(chart_snapshot) = self.chart_deck.take_pending_save() {
            self.persist_chart_snapshot(chart_snapshot);
        }

        if let Some(chart_id) = self.chart_deck.take_pending_delete() {
            self.delete_chart_snapshot(chart_id);
        }

        // Keep UI responsive and allow the 5s scanner refresh.
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_refreshes_scanner() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut app = DashboardApp {
            theme: Theme::new(),
            active_tab: Tab::Scanner,
            scanner: ScannerPanel::new(),
            positions: PositionsPanel::new(),
            wallets: WalletPanel::new(),
            chart_deck: ChartDeckPanel::new(),
            last_scan: Instant::now() - Duration::from_secs(6),
            portfolio_balance: 1_000.0,
            mode: TradingMode::Paper,
            shared: None,
            runtime,
            status: None,
            master_password: String::new(),
        };
        let before = app.scanner.last_refreshed;
        app.tick();
        assert!(app.scanner.last_refreshed > before);
    }
}
