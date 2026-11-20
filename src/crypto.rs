//! Hashing and keys.
//!
//! SHA-256 makes the ids. Ed25519 makes the keys and signatures.
//! The public key, written in hex, is the account name on the ledger.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

use crate::DIFFICULTY_BITS;

/// Hash bytes and return 64 lowercase hex characters.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// Make a new key pair. Returns (private_key_hex, public_key_hex).
pub fn generate_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let private_key = hex::encode(signing_key.to_bytes());
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    (private_key, public_key)
}

/// Derive the public key from a private key.
pub fn public_from_private(private_hex: &str) -> Result<String, String> {
    let bytes = decode_32(private_hex, "private key")?;
    let signing_key = SigningKey::from_bytes(&bytes);
    let public_key = signing_key.verifying_key();
    Ok(hex::encode(public_key.to_bytes()))
}

/// True when `value` is a real 32-byte public key in lowercase hex.
pub fn is_pubkey(value: &str) -> bool {
    let bytes = match decode_32(value, "public key") {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    VerifyingKey::from_bytes(&bytes).is_ok()
}

/// Sign a message with the private key. Returns the signature in hex.
pub fn sign(private_hex: &str, message: &[u8]) -> Result<String, String> {
    let bytes = decode_32(private_hex, "private key")?;
    let signing_key = SigningKey::from_bytes(&bytes);
    let signature = signing_key.sign(message);
    Ok(hex::encode(signature.to_bytes()))
}

/// True when the signature was made by the private key for this public key.
pub fn verify(public_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    let public_bytes = match decode_32(public_hex, "public key") {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature_bytes = match decode_hex(signature_hex, 64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key = match VerifyingKey::from_bytes(&public_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    public_key.verify(message, &signature).is_ok()
}

/// True when the hash is small enough to count as proof of work.
/// With 16 difficulty bits, the first two bytes must be 0.
pub fn under_target(hash_hex: &str) -> bool {
    let bytes = match decode_hex(hash_hex, 32) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let zero_bytes = (DIFFICULTY_BITS / 8) as usize;
    let mut i = 0;
    while i < zero_bytes {
        if bytes[i] != 0 {
            return false;
        }
        i += 1;
    }

    let extra_bits = DIFFICULTY_BITS % 8;
    if extra_bits == 0 {
        return true;
    }
    let shift = 8 - extra_bits;
    bytes[zero_bytes] >> shift == 0
}

fn decode_32(value: &str, label: &str) -> Result<[u8; 32], String> {
    let bytes = match decode_hex(value, 32) {
        Ok(bytes) => bytes,
        Err(_) => return Err(format!("bad {label}")),
    };
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = bytes[i];
        i += 1;
    }
    Ok(out)
}

fn decode_hex(value: &str, byte_len: usize) -> Result<Vec<u8>, String> {
    if value.len() != byte_len * 2 {
        return Err("bad hex".into());
    }
    for ch in value.chars() {
        if ch.is_ascii_uppercase() {
            return Err("bad hex".into());
        }
    }
    match hex::decode(value) {
        Ok(bytes) => Ok(bytes),
        Err(_) => Err("bad hex".into()),
    }
}
