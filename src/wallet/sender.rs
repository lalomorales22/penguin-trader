use crate::wallet::Network;

/// Transaction details for sending
#[derive(Debug, Clone)]
pub struct Transaction {
    pub tx_hash: String,
    pub network: Network,
    pub from_address: String,
    pub to_address: String,
    pub amount: f64,
    pub fee: f64,
    pub status: TransactionStatus,
    pub created_at: String,
}

/// Transaction status
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Confirmed,
    Failed,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Pending => write!(f, "PENDING"),
            TransactionStatus::Confirmed => write!(f, "CONFIRMED"),
            TransactionStatus::Failed => write!(f, "FAILED"),
        }
    }
}

/// Transaction sending error
#[derive(Debug)]
pub enum SendError {
    InvalidAddress(String),
    InsufficientBalance(String),
    InvalidAmount(String),
    SigningFailed(String),
    BroadcastFailed(String),
    RpcError(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            SendError::InsufficientBalance(msg) => write!(f, "Insufficient balance: {}", msg),
            SendError::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            SendError::SigningFailed(msg) => write!(f, "Signing failed: {}", msg),
            SendError::BroadcastFailed(msg) => write!(f, "Broadcast failed: {}", msg),
            SendError::RpcError(msg) => write!(f, "RPC error: {}", msg),
        }
    }
}

/// Send transaction on any supported network
pub async fn send_transaction(
    network: Network,
    from_address: &str,
    to_address: &str,
    amount: f64,
    private_key_hex: &str,
    fee: f64,
) -> Result<Transaction, SendError> {
    // Validate inputs
    if amount <= 0.0 {
        return Err(SendError::InvalidAmount(
            "Amount must be greater than 0".to_string(),
        ));
    }

    if to_address.is_empty() {
        return Err(SendError::InvalidAddress(
            "Recipient address cannot be empty".to_string(),
        ));
    }

    match network {
        Network::Bitcoin => send_bitcoin(from_address, to_address, amount, private_key_hex, fee).await,
        Network::Ethereum => send_ethereum(from_address, to_address, amount, private_key_hex, fee).await,
        Network::Solana => send_solana(from_address, to_address, amount, private_key_hex, fee).await,
        Network::Ripple => send_xrp(from_address, to_address, amount, private_key_hex, fee).await,
        Network::Cardano => send_cardano(from_address, to_address, amount, private_key_hex, fee).await,
        Network::Bnb => send_bnb(from_address, to_address, amount, private_key_hex, fee).await,
    }
}

/// Check transaction status on blockchain
pub async fn check_transaction_status(
    network: Network,
    tx_hash: &str,
) -> Result<TransactionStatus, SendError> {
    match network {
        Network::Bitcoin => check_bitcoin_status(tx_hash).await,
        Network::Ethereum => check_evm_status(tx_hash, "https://ethereum-rpc.publicnode.com").await,
        Network::Solana => check_solana_status(tx_hash).await,
        Network::Ripple => check_xrp_status(tx_hash).await,
        Network::Cardano => check_cardano_status(tx_hash).await,
        Network::Bnb => check_evm_status(tx_hash, "https://bsc-dataseed.binance.org/").await,
    }
}

async fn check_bitcoin_status(_tx_hash: &str) -> Result<TransactionStatus, SendError> {
    Ok(TransactionStatus::Pending)
}

/// Generic EVM status check
async fn check_evm_status(tx_hash: &str, rpc_url: &str) -> Result<TransactionStatus, SendError> {
    if tx_hash.is_empty() {
        return Err(SendError::RpcError("Invalid transaction hash".to_string()));
    }

    let provider = Provider::<Http>::try_from(rpc_url)
        .map_err(|e| SendError::RpcError(format!("Provider error: {}", e)))?;

    let tx_hash_bytes = H256::from_str(tx_hash)
        .map_err(|e| SendError::RpcError(format!("Invalid hash format: {}", e)))?;

    let receipt = provider.get_transaction_receipt(tx_hash_bytes).await
        .map_err(|e| SendError::RpcError(format!("Failed to fetch receipt: {}", e)))?;

    match receipt {
        Some(r) => {
            if r.status == Some(U64::from(1)) {
                Ok(TransactionStatus::Confirmed)
            } else {
                Ok(TransactionStatus::Failed)
            }
        }
        None => Ok(TransactionStatus::Pending),
    }
}

async fn check_solana_status(_tx_hash: &str) -> Result<TransactionStatus, SendError> {
    Ok(TransactionStatus::Pending)
}

async fn check_xrp_status(_tx_hash: &str) -> Result<TransactionStatus, SendError> {
    Ok(TransactionStatus::Pending)
}

async fn check_cardano_status(_tx_hash: &str) -> Result<TransactionStatus, SendError> {
    Ok(TransactionStatus::Pending)
}

/// Send Bitcoin transaction (simplified - uses API in production)
async fn send_bitcoin(
    from_address: &str,
    to_address: &str,
    amount: f64,
    _private_key_hex: &str,
    fee: f64,
) -> Result<Transaction, SendError> {
    // In production, use bitcoin-core RPC or a library like bitcoincore_rpc
    // This is a simplified demo that would call a Bitcoin node

    // Validate BTC address format (simplified)
    if !to_address.starts_with("bc1") && !to_address.starts_with("1") && !to_address.starts_with("3") {
        return Err(SendError::InvalidAddress("Invalid Bitcoin address format".to_string()));
    }

    // Simulate transaction signing and broadcast
    let tx_hash = format!("btc_{}", hex::encode(&rand::random::<[u8; 32]>()));

    Ok(Transaction {
        tx_hash,
        network: Network::Bitcoin,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee,
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

use ethers::prelude::*;
use std::convert::TryFrom;
use std::str::FromStr;

/// Send Ethereum transaction
async fn send_ethereum(
    from_address: &str,
    to_address: &str,
    amount: f64,
    private_key_hex: &str,
    _fee: f64, // Fee is estimated dynamically by the provider
) -> Result<Transaction, SendError> {
    // Validate ETH address format
    if !to_address.starts_with("0x") || to_address.len() != 42 {
        return Err(SendError::InvalidAddress("Invalid Ethereum address format".to_string()));
    }

    // Connect to provider (Infura for ETH Mainnet)
    // In a real app, use an env var for the project ID. For this demo, we'll use a public RPC or a placeholder.
    // Using a public RPC for mainnet (rate limits apply)
    
    // Check for test mode
    if std::env::var("PENGUIN_TEST_MODE").is_ok() {
        let tx_hash = format!("0x{}", hex::encode(&rand::random::<[u8; 32]>()));
        return Ok(Transaction {
            tx_hash,
            network: Network::Ethereum,
            from_address: from_address.to_string(),
            to_address: to_address.to_string(),
            amount,
            fee: 0.0,
            status: TransactionStatus::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    let provider = Provider::<Http>::try_from("https://ethereum-rpc.publicnode.com")
        .map_err(|e| SendError::RpcError(format!("Provider error: {}", e)))?;

    // Create wallet from private key
    let wallet: LocalWallet = private_key_hex
        .parse::<LocalWallet>()
        .map_err(|e| SendError::SigningFailed(format!("Invalid private key: {}", e)))?
        .with_chain_id(1u64); // Mainnet

    let client = SignerMiddleware::new(provider, wallet);

    // Parse amount (ETH -> Wei)
    let amount_wei = ethers::utils::parse_ether(amount)
        .map_err(|e| SendError::InvalidAmount(format!("Failed to parse amount: {}", e)))?;

    // Parse to address
    let to = Address::from_str(to_address)
        .map_err(|e| SendError::InvalidAddress(format!("Invalid to address: {}", e)))?;

    // Create transaction
    let tx = TransactionRequest::new()
        .to(to)
        .value(amount_wei);

    // Send transaction
    let pending_tx = client
        .send_transaction(tx, None)
        .await
        .map_err(|e| SendError::BroadcastFailed(format!("Failed to broadcast: {}", e)))?;

    let tx_hash = format!("{:?}", pending_tx.tx_hash());

    Ok(Transaction {
        tx_hash,
        network: Network::Ethereum,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee: 0.0, // Actual fee known after confirmation
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Send BNB transaction
async fn send_bnb(
    from_address: &str,
    to_address: &str,
    amount: f64,
    private_key_hex: &str,
    _fee: f64,
) -> Result<Transaction, SendError> {
    // Validate address
    if !to_address.starts_with("0x") || to_address.len() != 42 {
        return Err(SendError::InvalidAddress("Invalid BNB address format".to_string()));
    }

    // Connect to BSC Provider
    let provider = Provider::<Http>::try_from("https://bsc-dataseed.binance.org/")
        .map_err(|e| SendError::RpcError(format!("Provider error: {}", e)))?;

    // Create wallet
    let wallet: LocalWallet = private_key_hex
        .parse::<LocalWallet>()
        .map_err(|e| SendError::SigningFailed(format!("Invalid private key: {}", e)))?
        .with_chain_id(56u64); // BSC Mainnet

    let client = SignerMiddleware::new(provider, wallet);

    // Parse amount (BNB -> Wei)
    let amount_wei = ethers::utils::parse_ether(amount)
        .map_err(|e| SendError::InvalidAmount(format!("Failed to parse amount: {}", e)))?;

    let to = Address::from_str(to_address)
        .map_err(|e| SendError::InvalidAddress(format!("Invalid to address: {}", e)))?;

    let tx = TransactionRequest::new()
        .to(to)
        .value(amount_wei);

    let pending_tx = client
        .send_transaction(tx, None)
        .await
        .map_err(|e| SendError::BroadcastFailed(format!("Failed to broadcast: {}", e)))?;

    let tx_hash = format!("{:?}", pending_tx.tx_hash());

    Ok(Transaction {
        tx_hash,
        network: Network::Bnb,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee: 0.0,
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Send Solana transaction
async fn send_solana(
    from_address: &str,
    to_address: &str,
    amount: f64,
    _private_key_hex: &str,
    fee: f64,
) -> Result<Transaction, SendError> {
    // Validate Solana address format (base58, ~44 chars)
    if from_address.len() < 30 || from_address.len() > 50 {
        return Err(SendError::InvalidAddress(
            "Invalid Solana sender address".to_string(),
        ));
    }

    if to_address.len() < 30 || to_address.len() > 50 {
        return Err(SendError::InvalidAddress(
            "Invalid Solana recipient address".to_string(),
        ));
    }

    // In production, use solana-sdk to create and sign transactions
    let tx_hash = bs58::encode(&rand::random::<[u8; 32]>()).into_string();

    Ok(Transaction {
        tx_hash,
        network: Network::Solana,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee,
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Send XRP transaction
async fn send_xrp(
    from_address: &str,
    to_address: &str,
    amount: f64,
    _private_key_hex: &str,
    fee: f64,
) -> Result<Transaction, SendError> {
    // Validate XRP address format (starts with 'r')
    if !from_address.starts_with("r") {
        return Err(SendError::InvalidAddress("Invalid XRP sender address".to_string()));
    }

    if !to_address.starts_with("r") {
        return Err(SendError::InvalidAddress(
            "Invalid XRP recipient address".to_string(),
        ));
    }

    if from_address.len() < 25 || from_address.len() > 35 {
        return Err(SendError::InvalidAddress("Invalid XRP address length".to_string()));
    }

    // In production, use xrpl-rs library
    let tx_hash = hex::encode(&rand::random::<[u8; 32]>());

    Ok(Transaction {
        tx_hash,
        network: Network::Ripple,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee,
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Send Cardano transaction
async fn send_cardano(
    from_address: &str,
    to_address: &str,
    amount: f64,
    _private_key_hex: &str,
    fee: f64,
) -> Result<Transaction, SendError> {
    // Validate Cardano address format (starts with 'addr')
    if !from_address.starts_with("addr1") && !from_address.starts_with("addr") {
        return Err(SendError::InvalidAddress(
            "Invalid Cardano sender address".to_string(),
        ));
    }

    if !to_address.starts_with("addr1") && !to_address.starts_with("addr") {
        return Err(SendError::InvalidAddress(
            "Invalid Cardano recipient address".to_string(),
        ));
    }

    // In production, use cardano-serialization-lib
    let tx_hash = hex::encode(&rand::random::<[u8; 32]>());

    Ok(Transaction {
        tx_hash,
        network: Network::Cardano,
        from_address: from_address.to_string(),
        to_address: to_address.to_string(),
        amount,
        fee,
        status: TransactionStatus::Pending,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_status_display() {
        assert_eq!(TransactionStatus::Pending.to_string(), "PENDING");
        assert_eq!(TransactionStatus::Confirmed.to_string(), "CONFIRMED");
        assert_eq!(TransactionStatus::Failed.to_string(), "FAILED");
    }

    #[tokio::test]
    async fn test_send_bitcoin_validation() {
        let result = send_bitcoin(
            "1A1z7agoat7qriF8vV4eAKRYnYqZ8fvxdH",
            "invalid_address",
            1.0,
            "privatekey",
            0.0001,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_ethereum_validation() {
        let result = send_ethereum(
            "0x1234567890123456789012345678901234567890",
            "invalid_eth",
            1.0,
            "privatekey",
            0.001,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_solana_validation() {
        let result = send_solana(
            "invalid_sol",
            "invalid_sol",
            1.0,
            "privatekey",
            0.000005,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_zero_amount_rejected() {
        let result = send_transaction(
            Network::Bitcoin,
            "1A1z7agoat7qriF8vV4eAKRYnYqZ8fvxdH",
            "bc1qtest",
            0.0,
            "privatekey",
            0.0001,
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(SendError::InvalidAmount(_)) => (),
            _ => panic!("Expected InvalidAmount error"),
        }
    }

    #[tokio::test]
    async fn test_empty_recipient_rejected() {
        let result = send_transaction(
            Network::Ethereum,
            "0x1234567890123456789012345678901234567890",
            "",
            1.0,
            "privatekey",
            0.001,
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(SendError::InvalidAddress(_)) => (),
            _ => panic!("Expected InvalidAddress error"),
        }
    }
}
