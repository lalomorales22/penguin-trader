use rusqlite::{Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use serde_json;

use super::{Network, Wallet};

/// Chart datapoint persisted for the chart deck.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartPoint {
    pub timestamp: i64,
    pub price: f64,
    pub volume: Option<f64>,
}

/// Saved chart view mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChartView {
    Line,
    Bar,
}

/// Saved chart snapshot record stored in the shared wallet DB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartSnapshot {
    pub id: String,
    pub asset: String,
    pub wallet_label: Option<String>,
    pub overlays: Vec<String>,
    pub view: ChartView,
    pub mode: String,
    pub data: Vec<ChartPoint>,
    pub created_at: String,
    pub updated_at: String,
}

/// SQLite wallet storage manager
pub struct WalletStorage {
    db: Connection,
}

impl WalletStorage {
    /// Create or open wallet database
    pub fn new(db_path: &str) -> SqlResult<Self> {
        let db = Connection::open(db_path)?;
        let storage = WalletStorage { db };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> SqlResult<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS wallets (
                id TEXT PRIMARY KEY,
                network TEXT NOT NULL,
                address TEXT NOT NULL UNIQUE,
                encrypted_private_key BLOB NOT NULL,
                encrypted_mnemonic BLOB,
                encryption_salt BLOB NOT NULL,
                encryption_nonce BLOB NOT NULL,
                label TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS wallet_transactions (
                id INTEGER PRIMARY KEY,
                wallet_id TEXT NOT NULL,
                tx_hash TEXT NOT NULL,
                tx_type TEXT NOT NULL,
                amount REAL NOT NULL,
                to_address TEXT,
                from_address TEXT,
                fee REAL,
                status TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(wallet_id) REFERENCES wallets(id)
            );

            CREATE TABLE IF NOT EXISTS chart_snapshots (
                id TEXT PRIMARY KEY,
                asset TEXT NOT NULL,
                wallet_label TEXT,
                overlays TEXT NOT NULL,
                view_mode TEXT NOT NULL,
                mode TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    /// Save a wallet to the database
    pub fn save_wallet(&self, wallet: &Wallet) -> SqlResult<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO wallets
             (id, network, address, encrypted_private_key, encrypted_mnemonic,
              encryption_salt, encryption_nonce, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                wallet.id,
                serde_json::to_string(&wallet.network).unwrap_or_default(),
                wallet.address,
                wallet.encrypted_private_key,
                wallet.encrypted_mnemonic.as_ref(),
                wallet.encryption_salt,
                wallet.encryption_nonce,
                wallet.label,
                wallet.created_at,
                wallet.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Load a wallet by address
    pub fn load_wallet(&self, address: &str) -> SqlResult<Option<Wallet>> {
        let mut stmt = self.db.prepare(
            "SELECT id, network, address, encrypted_private_key, encrypted_mnemonic,
                    encryption_salt, encryption_nonce, label, created_at, updated_at
             FROM wallets WHERE address = ?1",
        )?;

        let wallet = stmt.query_row(rusqlite::params![address], |row| {
            let network_str: String = row.get(1)?;
            let network = serde_json::from_str(&network_str).unwrap_or(Network::Bitcoin);

            Ok(Wallet {
                id: row.get(0)?,
                network,
                address: row.get(2)?,
                encrypted_private_key: row.get(3)?,
                encrypted_mnemonic: row.get(4)?,
                encryption_salt: row.get(5)?,
                encryption_nonce: row.get(6)?,
                label: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        });

        match wallet {
            Ok(w) => Ok(Some(w)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Load all wallets for a network
    pub fn load_wallets_by_network(&self, network: Network) -> SqlResult<Vec<Wallet>> {
        let network_str = serde_json::to_string(&network).unwrap_or_default();

        let mut stmt = self.db.prepare(
            "SELECT id, network, address, encrypted_private_key, encrypted_mnemonic,
                    encryption_salt, encryption_nonce, label, created_at, updated_at
             FROM wallets WHERE network = ?1 ORDER BY created_at DESC",
        )?;

        let wallets = stmt.query_map(rusqlite::params![network_str], |row| {
            let network_str: String = row.get(1)?;
            let network = serde_json::from_str(&network_str).unwrap_or(Network::Bitcoin);

            Ok(Wallet {
                id: row.get(0)?,
                network,
                address: row.get(2)?,
                encrypted_private_key: row.get(3)?,
                encrypted_mnemonic: row.get(4)?,
                encryption_salt: row.get(5)?,
                encryption_nonce: row.get(6)?,
                label: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;

        let mut result = Vec::new();
        for wallet in wallets {
            result.push(wallet?);
        }
        Ok(result)
    }

    /// Load all wallets
    pub fn load_all_wallets(&self) -> SqlResult<Vec<Wallet>> {
        let mut stmt = self.db.prepare(
            "SELECT id, network, address, encrypted_private_key, encrypted_mnemonic,
                    encryption_salt, encryption_nonce, label, created_at, updated_at
             FROM wallets ORDER BY created_at DESC",
        )?;

        let wallets = stmt.query_map([], |row| {
            let network_str: String = row.get(1)?;
            let network = serde_json::from_str(&network_str).unwrap_or(Network::Bitcoin);

            Ok(Wallet {
                id: row.get(0)?,
                network,
                address: row.get(2)?,
                encrypted_private_key: row.get(3)?,
                encrypted_mnemonic: row.get(4)?,
                encryption_salt: row.get(5)?,
                encryption_nonce: row.get(6)?,
                label: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;

        let mut result = Vec::new();
        for wallet in wallets {
            result.push(wallet?);
        }
        Ok(result)
    }

    /// Delete a wallet by address
    pub fn delete_wallet(&self, address: &str) -> SqlResult<bool> {
        let affected = self.db.execute(
            "DELETE FROM wallets WHERE address = ?1",
            rusqlite::params![address],
        )?;
        Ok(affected > 0)
    }

    /// Check if a wallet exists
    pub fn wallet_exists(&self, address: &str) -> SqlResult<bool> {
        let mut stmt = self.db.prepare("SELECT 1 FROM wallets WHERE address = ?1 LIMIT 1")?;
        let exists = stmt.exists(rusqlite::params![address])?;
        Ok(exists)
    }

    /// Get wallet count
    pub fn wallet_count(&self) -> SqlResult<usize> {
        let mut stmt = self.db.prepare("SELECT COUNT(*) FROM wallets")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Save or update a chart snapshot to the shared DB.
    pub fn save_chart_snapshot(&self, snapshot: &ChartSnapshot) -> SqlResult<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO chart_snapshots
            (id, asset, wallet_label, overlays, view_mode, mode, data, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                snapshot.id,
                snapshot.asset,
                snapshot.wallet_label,
                serde_json::to_string(&snapshot.overlays).unwrap_or_default(),
                serde_json::to_string(&snapshot.view).unwrap_or_else(|_| "\"Line\"".to_string()),
                snapshot.mode,
                serde_json::to_string(&snapshot.data).unwrap_or_else(|_| "[]".to_string()),
                snapshot.created_at,
                snapshot.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Load all chart snapshots, optionally filtered by mode ("paper"/"live").
    pub fn load_chart_snapshots(&self, mode: Option<&str>) -> SqlResult<Vec<ChartSnapshot>> {
        let mut snapshots = Vec::new();
        if let Some(mode_str) = mode {
            let mut stmt = self.db.prepare(
                "SELECT id, asset, wallet_label, overlays, view_mode, mode, data, created_at, updated_at
                FROM chart_snapshots WHERE mode = ?1 ORDER BY updated_at DESC",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![mode_str], |row| Self::map_chart_row(row))?;
            for row in rows {
                snapshots.push(row?);
            }
        } else {
            let mut stmt = self.db.prepare(
                "SELECT id, asset, wallet_label, overlays, view_mode, mode, data, created_at, updated_at
                FROM chart_snapshots ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| Self::map_chart_row(row))?;
            for row in rows {
                snapshots.push(row?);
            }
        }
        Ok(snapshots)
    }

    /// Delete a chart snapshot by id. Returns true if a row was removed.
    pub fn delete_chart_snapshot(&self, id: &str) -> SqlResult<bool> {
        let affected = self.db.execute(
            "DELETE FROM chart_snapshots WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(affected > 0)
    }

    fn map_chart_row(row: &rusqlite::Row<'_>) -> SqlResult<ChartSnapshot> {
        let overlays_str: String = row.get(3)?;
        let view_str: String = row.get(4)?;
        let data_str: String = row.get(6)?;

        let overlays: Vec<String> =
            serde_json::from_str(&overlays_str).unwrap_or_else(|_| Vec::new());
        let view: ChartView = serde_json::from_str(&view_str).unwrap_or(ChartView::Line);
        let data: Vec<ChartPoint> =
            serde_json::from_str(&data_str).unwrap_or_else(|_| Vec::new());

        Ok(ChartSnapshot {
            id: row.get(0)?,
            asset: row.get(1)?,
            wallet_label: row.get(2)?,
            overlays,
            view,
            mode: row.get(5)?,
            data,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_storage() -> WalletStorage {
        WalletStorage::new(":memory:").expect("Failed to create test database")
    }

    #[test]
    fn test_save_and_load_wallet() {
        let storage = create_test_storage();

        let wallet = Wallet {
            id: "test_wallet_1".to_string(),
            network: Network::Bitcoin,
            address: "bc1qtest".to_string(),
            encrypted_private_key: vec![1, 2, 3, 4, 5],
            encrypted_mnemonic: None,
            encryption_salt: vec![6, 7, 8, 9, 10],
            encryption_nonce: vec![11, 12, 13, 14, 15],
            label: Some("My BTC Wallet".to_string()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        storage.save_wallet(&wallet).expect("Failed to save wallet");

        let loaded = storage
            .load_wallet("bc1qtest")
            .expect("Failed to load wallet");
        assert!(loaded.is_some());

        let loaded_wallet = loaded.unwrap();
        assert_eq!(loaded_wallet.address, "bc1qtest");
        assert_eq!(loaded_wallet.network, Network::Bitcoin);
        assert_eq!(loaded_wallet.label, Some("My BTC Wallet".to_string()));
    }

    #[test]
    fn test_wallet_count() {
        let storage = create_test_storage();

        assert_eq!(storage.wallet_count().unwrap(), 0);

        let wallet = Wallet {
            id: "test_wallet_1".to_string(),
            network: Network::Bitcoin,
            address: "bc1qtest1".to_string(),
            encrypted_private_key: vec![1, 2, 3],
            encrypted_mnemonic: None,
            encryption_salt: vec![4, 5, 6],
            encryption_nonce: vec![7, 8, 9],
            label: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        storage.save_wallet(&wallet).unwrap();
        assert_eq!(storage.wallet_count().unwrap(), 1);
    }

    #[test]
    fn test_delete_wallet() {
        let storage = create_test_storage();

        let wallet = Wallet {
            id: "test_wallet_1".to_string(),
            network: Network::Ethereum,
            address: "0xtest".to_string(),
            encrypted_private_key: vec![1, 2, 3],
            encrypted_mnemonic: None,
            encryption_salt: vec![4, 5, 6],
            encryption_nonce: vec![7, 8, 9],
            label: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        storage.save_wallet(&wallet).unwrap();
        assert!(storage.wallet_exists("0xtest").unwrap());

        let deleted = storage.delete_wallet("0xtest").unwrap();
        assert!(deleted);
        assert!(!storage.wallet_exists("0xtest").unwrap());
    }

    #[test]
    fn chart_snapshots_round_trip() {
        let storage = create_test_storage();
        let snapshot = ChartSnapshot {
            id: "chart_1".to_string(),
            asset: "BTCUSDT".to_string(),
            wallet_label: Some("Main".to_string()),
            overlays: vec!["RSI".to_string(), "Volume".to_string()],
            view: ChartView::Line,
            mode: "paper".to_string(),
            data: vec![
                ChartPoint {
                    timestamp: 1,
                    price: 100.0,
                    volume: Some(50.0),
                },
                ChartPoint {
                    timestamp: 2,
                    price: 101.5,
                    volume: None,
                },
            ],
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        storage.save_chart_snapshot(&snapshot).unwrap();
        let loaded = storage.load_chart_snapshots(Some("paper")).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].asset, "BTCUSDT");
        assert_eq!(loaded[0].view, ChartView::Line);

        let removed = storage.delete_chart_snapshot("chart_1").unwrap();
        assert!(removed);
        assert!(storage.load_chart_snapshots(None).unwrap().is_empty());
    }
}
