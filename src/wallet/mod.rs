pub mod encryption;
pub mod generator;
pub mod storage;
pub mod balance;
pub mod sender;

pub use encryption::{decrypt, encrypt, verify_password, EncryptedData};
pub use generator::{generate_mnemonic, generate_wallet, import_wallet_from_private_key};
pub use storage::{WalletStorage, ChartPoint, ChartSnapshot, ChartView};
pub use balance::{Balance, fetch_balance, estimate_fee};
pub use sender::{Transaction, TransactionStatus, send_transaction, check_transaction_status};

use serde::{Deserialize, Serialize};

/// Supported blockchain networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    Bitcoin,
    Ethereum,
    Solana,
    Ripple,
    Cardano,
    Bnb,
}

impl Network {
    pub fn symbol(&self) -> &str {
        match self {
            Network::Bitcoin => "BTC",
            Network::Ethereum => "ETH",
            Network::Solana => "SOL",
            Network::Ripple => "XRP",
            Network::Cardano => "ADA",
            Network::Bnb => "BNB",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Network::Bitcoin => "Bitcoin",
            Network::Ethereum => "Ethereum",
            Network::Solana => "Solana",
            Network::Ripple => "Ripple",
            Network::Cardano => "Cardano",
            Network::Bnb => "BNB Smart Chain",
        }
    }
}

/// Wallet structure for storing crypto addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub id: String,
    pub network: Network,
    pub address: String,
    pub encrypted_private_key: Vec<u8>,
    pub encrypted_mnemonic: Option<Vec<u8>>,
    pub encryption_salt: Vec<u8>,
    pub encryption_nonce: Vec<u8>,
    pub label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_symbols() {
        assert_eq!(Network::Bitcoin.symbol(), "BTC");
        assert_eq!(Network::Ethereum.symbol(), "ETH");
        assert_eq!(Network::Solana.symbol(), "SOL");
        assert_eq!(Network::Ripple.symbol(), "XRP");
        assert_eq!(Network::Cardano.symbol(), "ADA");
        assert_eq!(Network::Bnb.symbol(), "BNB");
    }
}
