use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::SaltString;
use rand::Rng;

/// Encryption key size for AES-256
const KEY_SIZE: usize = 32;

/// Nonce size for AES-GCM (96-bit / 12 bytes)
const NONCE_SIZE: usize = 12;

/// Salt size for Argon2
const SALT_SIZE: usize = 16;

/// Encryption result with salt and nonce for storage
#[derive(Debug, Clone)]
pub struct EncryptedData {
    /// Encrypted ciphertext
    pub ciphertext: Vec<u8>,
    /// Salt used for key derivation
    pub salt: Vec<u8>,
    /// Nonce used for encryption
    pub nonce: Vec<u8>,
}

/// Key derivation using Argon2id
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_SIZE], String> {
    // Use Argon2 password hashing
    let salt_string = SaltString::encode_b64(salt)
        .map_err(|e| format!("Salt encoding failed: {}", e))?;

    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|e| format!("Argon2 hashing failed: {}", e))?
        .hash
        .ok_or("Hash output missing")?;

    let hash_bytes = hash.as_bytes();
    if hash_bytes.len() < KEY_SIZE {
        return Err("Hash output too small for key derivation".to_string());
    }

    let mut key = [0u8; KEY_SIZE];
    key.copy_from_slice(&hash_bytes[..KEY_SIZE]);
    Ok(key)
}

/// Encrypt data with AES-256-GCM using a password
pub fn encrypt(data: &[u8], password: &str) -> Result<EncryptedData, String> {
    let mut rng = rand::thread_rng();

    // Generate random salt
    let mut salt = vec![0u8; SALT_SIZE];
    rng.fill(&mut salt[..]);

    // Derive encryption key from password and salt
    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new((&key).into());

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the data
    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok(EncryptedData {
        ciphertext,
        salt,
        nonce: nonce_bytes.to_vec(),
    })
}

/// Decrypt data with AES-256-GCM using a password
pub fn decrypt(encrypted: &EncryptedData, password: &str) -> Result<Vec<u8>, String> {
    // Derive decryption key from password and stored salt
    let key = derive_key(password, &encrypted.salt)?;
    let cipher = Aes256Gcm::new((&key).into());

    // Decrypt using stored nonce
    let nonce = Nonce::from_slice(&encrypted.nonce);
    let plaintext = cipher
        .decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|e| format!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

/// Verify a password is correct by attempting to decrypt test data
pub fn verify_password(test_encrypted: &EncryptedData, password: &str) -> bool {
    decrypt(test_encrypted, password).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let data = b"This is a secret message";
        let password = "my_secure_password";

        let encrypted = encrypt(data, password).expect("Encryption failed");
        let decrypted = decrypt(&encrypted, password).expect("Decryption failed");

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_password() {
        let data = b"Secret data";
        let password = "correct_password";
        let wrong_password = "wrong_password";

        let encrypted = encrypt(data, password).expect("Encryption failed");
        let result = decrypt(&encrypted, wrong_password);

        assert!(result.is_err(), "Wrong password should fail decryption");
    }

    #[test]
    fn test_salt_uniqueness() {
        let data = b"Same data";
        let password = "same_password";

        let enc1 = encrypt(data, password).expect("Encryption 1 failed");
        let enc2 = encrypt(data, password).expect("Encryption 2 failed");

        // Different salts should produce different ciphertexts
        assert_ne!(enc1.salt, enc2.salt);
        assert_ne!(enc1.ciphertext, enc2.ciphertext);

        // But both should decrypt correctly
        let dec1 = decrypt(&enc1, password).expect("Decrypt 1 failed");
        let dec2 = decrypt(&enc2, password).expect("Decrypt 2 failed");

        assert_eq!(dec1, dec2);
    }
}
