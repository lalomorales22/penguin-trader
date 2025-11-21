use chrono::Utc;
use penguin_trader::wallet::{
    encrypt, fetch_balance, generate_wallet, send_transaction, Network, TransactionStatus,
    Wallet, WalletStorage,
};

/// End-to-end wallet flow in paper/test mode:
/// generate wallet -> encrypt/store -> fetch balance (mocked) -> send tx (mocked).
#[tokio::test]
async fn wallet_generate_balance_send_flow() {
    std::env::set_var("PENGUIN_TEST_MODE", "1");

    let network = Network::Ethereum;
    let (address, private_key, mnemonic) = generate_wallet(network).expect("wallet gen failed");

    // Encrypt the sensitive payload
    let payload = format!("{}|{}", private_key, mnemonic);
    let encrypted = encrypt(payload.as_bytes(), "test_password").expect("encryption failed");

    // Persist to in-memory storage
    let storage = WalletStorage::new(":memory:").expect("storage init failed");
    let wallet = Wallet {
        id: "integration_wallet".to_string(),
        network,
        address: address.clone(),
        encrypted_private_key: encrypted.ciphertext.clone(),
        encrypted_mnemonic: Some(encrypted.ciphertext.clone()),
        encryption_salt: encrypted.salt.clone(),
        encryption_nonce: encrypted.nonce.clone(),
        label: Some("Integration".to_string()),
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    storage.save_wallet(&wallet).expect("wallet save failed");

    // Deterministic balance/fee via PENGUIN_TEST_MODE
    let balance = fetch_balance(&address, network).await.expect("balance failed");
    assert!(balance.balance > 0.0, "balance should be non-zero in test mode");

    let fee = penguin_trader::wallet::estimate_fee(network, 0.1)
        .await
        .expect("fee failed");

    let tx = send_transaction(network, &address, &address, 0.1, &private_key, fee)
        .await
        .expect("send failed");

    assert_eq!(tx.network, network);
    assert_eq!(tx.status, TransactionStatus::Pending);
}
