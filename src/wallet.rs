//! Wallets.
//!
//! A name like "alice" is only on this computer.
//! The ledger stores the public key, never the private key.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::tx::Tx;

/// What gets written to wallets.json.
#[derive(Clone, Serialize, Deserialize)]
struct StoredWallet {
    private: String,
    public: String,
}

/// One key pair. The private key stays in this struct.
#[derive(Clone)]
pub struct Wallet {
    pub name: String,
    private: String,
    pub public: String,
}

impl Wallet {
    /// Build a signed payment from this wallet to `to`.
    pub fn sign_payment(&self, to: &str, amount: u64, nonce: u64) -> Result<Tx, String> {
        if !crypto::is_pubkey(to) {
            return Err("invalid public key".into());
        }
        if amount == 0 {
            return Err("invalid amount".into());
        }

        let public_key = crypto::public_from_private(&self.private)?;
        if public_key != self.public {
            return Err("wallet public key does not match private key".into());
        }

        let mut tx = Tx {
            from: public_key,
            to: to.to_string(),
            amount,
            nonce,
            signature: String::new(),
        };
        tx.signature = crypto::sign(&self.private, &tx.signing_bytes())?;
        Ok(tx)
    }
}

/// All of the wallets saved in one file.
pub struct Wallets {
    path: PathBuf,
    keys: HashMap<String, Wallet>,
}

impl Wallets {
    pub fn open(path: &Path) -> Result<Wallets, String> {
        if !path.exists() {
            return Ok(Wallets {
                path: path.to_path_buf(),
                keys: HashMap::new(),
            });
        }

        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => return Err(error.to_string()),
        };
        let stored: HashMap<String, StoredWallet> = match serde_json::from_str(&text) {
            Ok(stored) => stored,
            Err(error) => return Err(format!("corrupt wallet file: {error}")),
        };

        let mut keys = HashMap::new();
        for (name, entry) in stored {
            check_name(&name)?;
            let public_key = crypto::public_from_private(&entry.private)?;
            if public_key != entry.public {
                return Err(format!(
                    "wallet {name} public key does not match private key"
                ));
            }
            keys.insert(
                name.clone(),
                Wallet {
                    name,
                    private: entry.private,
                    public: public_key,
                },
            );
        }

        Ok(Wallets {
            path: path.to_path_buf(),
            keys,
        })
    }

    /// Create a new key pair and save it under `name`.
    pub fn create(&mut self, name: &str) -> Result<Wallet, String> {
        check_name(name)?;
        if self.keys.contains_key(name) {
            return Err(format!("wallet {name} already exists"));
        }

        let (private_key, public_key) = crypto::generate_keypair();
        let wallet = Wallet {
            name: name.to_string(),
            private: private_key,
            public: public_key,
        };
        self.keys.insert(name.to_string(), wallet.clone());
        self.save()?;
        Ok(wallet)
    }

    pub fn get(&self, name: &str) -> Result<&Wallet, String> {
        match self.keys.get(name) {
            Some(wallet) => Ok(wallet),
            None => Err(format!("no wallet named {name}")),
        }
    }

    /// A wallet name on this computer, or a public key written in hex.
    pub fn resolve(&self, name_or_key: &str) -> Result<String, String> {
        if let Some(wallet) = self.keys.get(name_or_key) {
            return Ok(wallet.public.clone());
        }
        if crypto::is_pubkey(name_or_key) {
            return Ok(name_or_key.to_string());
        }
        Err(format!("no wallet named {name_or_key}"))
    }

    /// Wallet name if we know this public key, otherwise the key itself.
    pub fn label(&self, public_key: &str) -> String {
        for wallet in self.keys.values() {
            if wallet.public == public_key {
                return wallet.name.clone();
            }
        }
        public_key.to_string()
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Err(error.to_string());
            }
        }

        let mut stored = HashMap::new();
        for (name, wallet) in &self.keys {
            stored.insert(
                name.clone(),
                StoredWallet {
                    private: wallet.private.clone(),
                    public: wallet.public.clone(),
                },
            );
        }
        let text = match serde_json::to_string_pretty(&stored) {
            Ok(text) => text,
            Err(error) => return Err(error.to_string()),
        };
        match fs::write(&self.path, text + "\n") {
            Ok(()) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 32 {
        return Err("bad wallet name".into());
    }
    for ch in name.chars() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if !ok {
            return Err("bad wallet name".into());
        }
    }
    Ok(())
}
