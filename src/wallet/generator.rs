use rand::Rng;
use sha2::{Digest, Sha256};
use hex;

use super::Network;

/// Generate a 12-word BIP39 mnemonic phrase (simplified version)
/// In production, use a proper BIP39 library
pub fn generate_mnemonic() -> Result<String, String> {
    let mut rng = rand::thread_rng();

    // For simplified demo, generate 12 random words from a small wordlist
    let words = vec![
        "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
        "academy", "accent", "accept", "access", "accident", "account", "achieve", "acid",
        "acoustic", "acquire", "across", "act", "action", "active", "activity", "actor",
    ];

    let mut mnemonic_words = Vec::new();
    for _ in 0..12 {
        let idx = rng.gen_range(0..words.len());
        mnemonic_words.push(words[idx]);
    }

    Ok(mnemonic_words.join(" "))
}

/// Generate a random private key (simplified)
fn generate_private_key() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut key = vec![0u8; 32];
    rng.fill(&mut key[..]);
    key
}

/// Derive a simple public key from private key using SHA256
fn derive_public_key(private_key: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(private_key);
    hasher.finalize().to_vec()
}

/// Bitcoin wallet generation
pub fn generate_bitcoin_wallet() -> Result<(String, String), String> {
    let private_key = generate_private_key();
    let public_key = derive_public_key(&private_key);

    // Generate a fake but valid-looking Bitcoin SegWit address
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("bc1{}", hex::encode(&hash[..20]));
    let private_key_hex = hex::encode(&private_key);

    Ok((address, private_key_hex))
}

/// Ethereum wallet generation (also used for BNB on BSC)
pub fn generate_ethereum_wallet() -> Result<(String, String), String> {
    let private_key = generate_private_key();
    let public_key = derive_public_key(&private_key);

    // Generate Ethereum-style address
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("0x{}", hex::encode(&hash[hash.len() - 20..]));
    let private_key_hex = hex::encode(&private_key);

    Ok((address, private_key_hex))
}

/// Solana wallet generation
pub fn generate_solana_wallet() -> Result<(String, String), String> {
    let private_key = generate_private_key();
    let public_key = derive_public_key(&private_key);

    // Solana address is base58 encoded public key
    let address = bs58::encode(&public_key).into_string();
    let private_key_hex = hex::encode(&private_key);

    Ok((address, private_key_hex))
}

/// XRP (Ripple) wallet generation
pub fn generate_xrp_wallet() -> Result<(String, String), String> {
    let private_key = generate_private_key();
    let public_key = derive_public_key(&private_key);

    // XRP address starts with 'r' and is derived from public key hash
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address_data = [&[0x05][..], &hash[..20]].concat();
    let address = format!("r{}", bs58::encode(&address_data).into_string());
    let private_key_hex = hex::encode(&private_key);

    Ok((address, private_key_hex))
}

/// Cardano wallet generation
pub fn generate_cardano_wallet() -> Result<(String, String), String> {
    let private_key = generate_private_key();
    let public_key = derive_public_key(&private_key);

    // Cardano address uses bech32 encoding
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("addr1{}", hex::encode(&hash[..28]));
    let private_key_hex = hex::encode(&private_key);

    Ok((address, private_key_hex))
}

/// BNB wallet generation (same as Ethereum)
pub fn generate_bnb_wallet() -> Result<(String, String), String> {
    generate_ethereum_wallet()
}

/// Generate wallet for any supported network
pub fn generate_wallet(
    network: Network,
) -> Result<(String, String, String), String> {
    let (address, private_key) = match network {
        Network::Bitcoin => generate_bitcoin_wallet()?,
        Network::Ethereum => generate_ethereum_wallet()?,
        Network::Solana => generate_solana_wallet()?,
        Network::Ripple => generate_xrp_wallet()?,
        Network::Cardano => generate_cardano_wallet()?,
        Network::Bnb => generate_bnb_wallet()?,
    };

    let mnemonic = generate_mnemonic()?;
    Ok((address, private_key, mnemonic))
}

/// Import wallet from private key for Bitcoin
pub fn import_bitcoin_wallet(private_key_hex: &str) -> Result<String, String> {
    let private_key = hex::decode(private_key_hex)
        .map_err(|_| "Invalid hex format for Bitcoin private key".to_string())?;

    if private_key.len() != 32 {
        return Err("Bitcoin private key must be 32 bytes".to_string());
    }

    let public_key = derive_public_key(&private_key);
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("bc1{}", hex::encode(&hash[..20]));
    Ok(address)
}

/// Import wallet from private key for Ethereum/BNB
pub fn import_ethereum_wallet(private_key_hex: &str) -> Result<String, String> {
    let private_key_hex = if private_key_hex.starts_with("0x") {
        &private_key_hex[2..]
    } else {
        private_key_hex
    };

    let private_key = hex::decode(private_key_hex)
        .map_err(|_| "Invalid hex format for Ethereum private key".to_string())?;

    if private_key.len() != 32 {
        return Err("Ethereum private key must be 32 bytes".to_string());
    }

    let public_key = derive_public_key(&private_key);
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("0x{}", hex::encode(&hash[hash.len() - 20..]));
    Ok(address)
}

/// Import wallet from private key for Solana
pub fn import_solana_wallet(private_key_hex: &str) -> Result<String, String> {
    let private_key = hex::decode(private_key_hex)
        .map_err(|_| "Invalid hex format for Solana private key".to_string())?;

    if private_key.len() != 32 {
        return Err("Solana private key must be 32 bytes".to_string());
    }

    let public_key = derive_public_key(&private_key);
    let address = bs58::encode(&public_key).into_string();
    Ok(address)
}

/// Import wallet from private key for XRP
pub fn import_xrp_wallet(private_key_hex: &str) -> Result<String, String> {
    let private_key = hex::decode(private_key_hex)
        .map_err(|_| "Invalid hex format for XRP private key".to_string())?;

    if private_key.len() != 32 {
        return Err("XRP private key must be 32 bytes".to_string());
    }

    let public_key = derive_public_key(&private_key);
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address_data = [&[0x05][..], &hash[..20]].concat();
    let address = format!("r{}", bs58::encode(&address_data).into_string());
    Ok(address)
}

/// Import wallet from private key for Cardano
pub fn import_cardano_wallet(private_key_hex: &str) -> Result<String, String> {
    let private_key = hex::decode(private_key_hex)
        .map_err(|_| "Invalid hex format for Cardano private key".to_string())?;

    if private_key.len() != 32 {
        return Err("Cardano private key must be 32 bytes".to_string());
    }

    let public_key = derive_public_key(&private_key);
    let mut hasher = Sha256::new();
    hasher.update(&public_key);
    let hash = hasher.finalize();

    let address = format!("addr1{}", hex::encode(&hash[..28]));
    Ok(address)
}

/// Import wallet from private key for any supported network
pub fn import_wallet_from_private_key(
    network: Network,
    private_key: &str,
) -> Result<String, String> {
    match network {
        Network::Bitcoin => import_bitcoin_wallet(private_key),
        Network::Ethereum => import_ethereum_wallet(private_key),
        Network::Solana => import_solana_wallet(private_key),
        Network::Ripple => import_xrp_wallet(private_key),
        Network::Cardano => import_cardano_wallet(private_key),
        Network::Bnb => import_ethereum_wallet(private_key), // BNB uses Ethereum format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic() {
        let result = generate_mnemonic();
        assert!(result.is_ok());

        let mnemonic = result.unwrap();
        let words: Vec<&str> = mnemonic.split_whitespace().collect();
        assert_eq!(words.len(), 12);
    }

    #[test]
    fn test_generate_bitcoin_wallet() {
        let result = generate_bitcoin_wallet();
        assert!(result.is_ok());

        let (address, private_key) = result.unwrap();
        assert!(address.starts_with("bc1"));
        assert!(!private_key.is_empty());
        assert_eq!(private_key.len(), 64); // 32 bytes in hex
    }

    #[test]
    fn test_generate_ethereum_wallet() {
        let result = generate_ethereum_wallet();
        assert!(result.is_ok());

        let (address, private_key) = result.unwrap();
        assert!(address.starts_with("0x"));
        assert_eq!(address.len(), 42); // 0x + 40 hex chars
        assert!(!private_key.is_empty());
    }

    #[test]
    fn test_generate_solana_wallet() {
        let result = generate_solana_wallet();
        assert!(result.is_ok());

        let (address, private_key) = result.unwrap();
        assert!(!address.is_empty());
        assert!(!private_key.is_empty());
    }

    #[test]
    fn test_generate_xrp_wallet() {
        let result = generate_xrp_wallet();
        assert!(result.is_ok());

        let (address, private_key) = result.unwrap();
        assert!(address.starts_with("r"));
        assert!(!private_key.is_empty());
    }

    #[test]
    fn test_generate_cardano_wallet() {
        let result = generate_cardano_wallet();
        assert!(result.is_ok());

        let (address, private_key) = result.unwrap();
        assert!(address.starts_with("addr1"));
        assert!(!private_key.is_empty());
    }

    #[test]
    fn test_generate_wallet_all_networks() {
        let networks = vec![
            Network::Bitcoin,
            Network::Ethereum,
            Network::Solana,
            Network::Ripple,
            Network::Cardano,
            Network::Bnb,
        ];

        for network in networks {
            let result = generate_wallet(network);
            assert!(result.is_ok(), "Failed for {:?}", network);

            let (address, private_key, mnemonic) = result.unwrap();
            assert!(!address.is_empty(), "{:?} address is empty", network);
            assert!(!private_key.is_empty(), "{:?} private key is empty", network);
            let words: Vec<&str> = mnemonic.split_whitespace().collect();
            assert_eq!(words.len(), 12, "{:?} mnemonic doesn't have 12 words", network);
        }
    }
}
