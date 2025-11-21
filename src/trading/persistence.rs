use std::fs;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::strategy::Action;
use crate::wallet::WalletStorage;

/// Position side stored in the shared JSON file so GUI/CLI can agree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PositionSide {
    LONG,
    SHORT,
}

impl PositionSide {
    pub fn as_action(&self) -> Action {
        match self {
            PositionSide::LONG => Action::LONG,
            PositionSide::SHORT => Action::SHORT,
        }
    }
}

/// Minimal open-position record persisted to penguin_data.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRecord {
    pub position_id: String,
    pub symbol: String,
    pub side: PositionSide,
    pub entry_price: f64,
    pub size: f64,
    pub leverage: u32,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_percent: f64,
    pub opened_at: String,
}

impl PositionRecord {
    /// Derive an approximate last price by backing into the unrealized PnL percent.
    pub fn approximate_last(&self) -> f64 {
        if self.leverage == 0 {
            return self.entry_price;
        }
        let pct = self.unrealized_pnl_percent / self.leverage as f64 / 100.0;
        match self.side {
            PositionSide::LONG => self.entry_price * (1.0 + pct),
            PositionSide::SHORT => self.entry_price * (1.0 - pct),
        }
    }
}

/// Closed trade record persisted in penguin_data.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTradeRecord {
    pub trade_id: String,
    pub symbol: String,
    pub position_type: PositionSide,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub leverage: u32,
    pub realized_pnl: f64,
    pub realized_pnl_percent: f64,
    pub hold_time_minutes: i64,
    pub opened_at: String,
    pub closed_at: String,
}

/// Portfolio snapshot stored alongside trades.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRecord {
    pub balance: f64,
    pub realized_pnl: f64,
    pub created_at: String,
}

/// Combined data structure that mirrors the CLI's penguin_data.json shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedAppData {
    pub portfolio: PortfolioRecord,
    pub open_positions: Vec<PositionRecord>,
    pub closed_trades: Vec<ClosedTradeRecord>,
}

impl SharedAppData {
    pub fn default_with_starting_balance(starting_balance: f64) -> Self {
        SharedAppData {
            portfolio: PortfolioRecord {
                balance: starting_balance,
                realized_pnl: 0.0,
                created_at: Utc::now().to_rfc3339(),
            },
            open_positions: Vec::new(),
            closed_trades: Vec::new(),
        }
    }
}

/// Shared store that gives GUI + CLI a single source of truth for wallets and trades.
pub struct SharedDataStore {
    wallet_path: String,
    pub wallet_db: WalletStorage,
    pub app_data_path: String,
}

impl SharedDataStore {
    /// Open/create the wallet database and remember the path to the app data file.
    pub fn new(wallet_path: impl Into<String>, app_data_path: impl Into<String>) -> Result<Self, String> {
        let wallet_path_str = wallet_path.into();
        let wallet_db = WalletStorage::new(&wallet_path_str)
            .map_err(|e| format!("Failed to open wallet db at {}: {}", wallet_path_str, e))?;
        let app_path = app_data_path.into();
        Ok(SharedDataStore {
            wallet_path: wallet_path_str,
            wallet_db,
            app_data_path: app_path,
        })
    }

    /// Build a mode-specific app data path (e.g., penguin_data_paper.json).
    fn mode_path(&self, mode: &str) -> String {
        if self.app_data_path.ends_with(".json") {
            let stem = self.app_data_path.trim_end_matches(".json");
            format!("{}_{}.json", stem, mode)
        } else {
            format!("{}_{}", self.app_data_path, mode)
        }
    }

    /// Public helper for UI status messages.
    pub fn app_data_path_for_mode(&self, mode: &str) -> String {
        self.mode_path(mode)
    }

    /// Read the penguin_data.json used by the CLI; fall back to a default structure.
    pub fn load_app_data(&self) -> SharedAppData {
        self.load_app_data_for_mode("paper")
    }

    /// Read mode-scoped app data, falling back to legacy single-file storage.
    pub fn load_app_data_for_mode(&self, mode: &str) -> SharedAppData {
        let mode_path = self.mode_path(mode);
        if let Ok(contents) = fs::read_to_string(&mode_path) {
            if let Ok(data) = serde_json::from_str::<SharedAppData>(&contents) {
                return data;
            }
        }

        // Legacy fallback: load from the base file when present (treated as paper mode).
        if mode == "paper" {
            if let Ok(contents) = fs::read_to_string(&self.app_data_path) {
                if let Ok(data) = serde_json::from_str::<SharedAppData>(&contents) {
                    return data;
                }
            }
        }

        SharedAppData::default_with_starting_balance(10_000.0)
    }

    /// Persist updated app data for GUI or CLI edits.
    pub fn save_app_data(&self, data: &SharedAppData) -> Result<(), String> {
        self.save_app_data_for_mode("paper", data)
    }

    /// Persist mode-scoped app data, writing to a mode-specific file.
    pub fn save_app_data_for_mode(&self, mode: &str, data: &SharedAppData) -> Result<(), String> {
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| format!("Failed to serialize shared app data: {}", e))?;
        let path = self.mode_path(mode);
        fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path, e))
    }

    pub fn wallet_path(&self) -> &str {
        &self.wallet_path
    }

    pub fn app_data_path(&self) -> &str {
        &self.app_data_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_data_uses_starting_balance() {
        let data = SharedAppData::default_with_starting_balance(5_000.0);
        assert_eq!(data.portfolio.balance, 5_000.0);
        assert!(data.open_positions.is_empty());
    }

    #[test]
    fn load_and_save_roundtrips() {
        let tmp_path = std::env::temp_dir().join("penguin_shared_app_data.json");
        let data = SharedAppData {
            portfolio: PortfolioRecord {
                balance: 12_345.0,
                realized_pnl: 50.0,
                created_at: Utc::now().to_rfc3339(),
            },
            open_positions: vec![PositionRecord {
                position_id: "pos_1".to_string(),
                symbol: "ETHUSDT".to_string(),
                side: PositionSide::LONG,
                entry_price: 2000.0,
                size: 1.5,
                leverage: 5,
                stop_loss: 1900.0,
                take_profit: 2300.0,
                unrealized_pnl: 0.0,
                unrealized_pnl_percent: 0.0,
                opened_at: Utc::now().to_rfc3339(),
            }],
            closed_trades: Vec::new(),
        };

        let store =
            SharedDataStore::new(":memory:", tmp_path.to_string_lossy().to_string()).expect("store init failed");
        store.save_app_data(&data).expect("save failed");

        let reloaded = store.load_app_data();
        assert_eq!(reloaded.portfolio.balance, 12_345.0);
        assert_eq!(reloaded.open_positions.len(), 1);

        let _ = fs::remove_file(store.app_data_path_for_mode("paper"));
    }

    #[test]
    fn approximate_last_uses_unrealized_pct() {
        let record = PositionRecord {
            position_id: "demo".to_string(),
            symbol: "SOLUSDT".to_string(),
            side: PositionSide::LONG,
            entry_price: 100.0,
            size: 1.0,
            leverage: 10,
            stop_loss: 90.0,
            take_profit: 120.0,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 50.0,
            opened_at: Utc::now().to_rfc3339(),
        };
        let implied_last = record.approximate_last();
        assert!(implied_last > 100.0);
    }

    #[test]
    fn mode_paths_append_suffix() {
        let base = "demo_data.json";
        let store = SharedDataStore::new(":memory:", base).expect("store init failed");
        assert!(store.app_data_path_for_mode("paper").ends_with("_paper.json"));
        assert!(store.app_data_path_for_mode("live").ends_with("_live.json"));
    }

    #[test]
    fn legacy_base_file_is_used_for_paper_mode() {
        let base = std::env::temp_dir().join("penguin_legacy.json");
        let legacy = SharedAppData::default_with_starting_balance(1_234.0);
        let json = serde_json::to_string_pretty(&legacy).unwrap();
        fs::write(&base, json).expect("write legacy file");

        let store =
            SharedDataStore::new(":memory:", base.to_string_lossy().to_string()).expect("store init failed");
        let loaded = store.load_app_data_for_mode("paper");
        assert_eq!(loaded.portfolio.balance, 1_234.0);

        let _ = fs::remove_file(store.app_data_path_for_mode("paper"));
        let _ = fs::remove_file(base);
    }
}
