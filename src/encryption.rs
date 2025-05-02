use crate::ENCRYPTION_KEY;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{Engine as _, engine::general_purpose};
use rand::{Rng, thread_rng};
use sha2::{Digest, Sha256};

const ENABLED_ENCRYPTION: bool = false;

pub fn encrypt_value(value: &str) -> String {
    // If encryption is disabled, return the value as is
    if !ENABLED_ENCRYPTION {
        return value.to_string();
    }

    // Generate a random 96-bit nonce (12 bytes)
    let mut nonce_bytes = [0u8; 12];
    thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Derive key from ENCRYPTION_KEY using SHA-256
    let mut hasher = Sha256::new();
    hasher.update(ENCRYPTION_KEY.as_bytes());
    let key_bytes = hasher.finalize();

    // Create cipher instance
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // Encrypt the value
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .expect("encryption failure");

    // Combine nonce and ciphertext and encode as base64
    let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    general_purpose::STANDARD.encode(result)
}

pub fn decrypt_value(value: &str) -> String {
    // If encryption is disabled, return the value as is
    if !ENABLED_ENCRYPTION {
        return value.to_string();
    }

    // Check if this is our old format placeholder
    if value.starts_with("encrypted(") && value.ends_with(")") {
        return value.replace("encrypted(", "").replace(")", "");
    }

    // Decode base64
    let decoded = match general_purpose::STANDARD.decode(value) {
        Ok(d) => d,
        Err(_) => return String::from("decryption error: invalid base64"),
    };

    // Need at least 12 bytes for the nonce
    if decoded.len() <= 12 {
        return String::from("decryption error: data too short");
    }

    // Extract nonce and ciphertext
    let nonce = Nonce::from_slice(&decoded[0..12]);
    let ciphertext = &decoded[12..];

    // Derive key from ENCRYPTION_KEY
    let mut hasher = Sha256::new();
    hasher.update(ENCRYPTION_KEY.as_bytes());
    let key_bytes = hasher.finalize();

    // Create cipher instance
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // Decrypt
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext)
            .unwrap_or_else(|_| String::from("decryption error: invalid utf8")),
        Err(_) => String::from("decryption error: authentication failed"),
    }
}
