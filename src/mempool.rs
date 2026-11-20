//! The mempool is the list of payments that are not in a block yet.
//!
//! A payment stays here until the miner puts it in the ledger.

use std::fs;
use std::path::{Path, PathBuf};

use crate::tx::{self, State, Tx};

pub struct Mempool {
    path: PathBuf,
    txs: Vec<Tx>,
}

impl Mempool {
    pub fn open(path: &Path) -> Result<Mempool, String> {
        let txs = if !path.exists() {
            Vec::new()
        } else {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => return Err(error.to_string()),
            };
            if text.trim().is_empty() {
                Vec::new()
            } else {
                match serde_json::from_str(&text) {
                    Ok(txs) => txs,
                    Err(error) => return Err(format!("corrupt mempool: {error}")),
                }
            }
        };
        Ok(Mempool {
            path: path.to_path_buf(),
            txs,
        })
    }

    pub fn txs(&self) -> &[Tx] {
        &self.txs
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.len() == 0
    }

    /// The nonce to use for this account's next payment.
    /// Payments already waiting in the pool count.
    pub fn next_nonce(&self, state: &State, account: &str) -> Result<u64, String> {
        let mut preview = state.clone();
        for tx in &self.txs {
            tx::apply_payment(&mut preview, tx)?;
        }
        match preview.nonces.get(account) {
            Some(nonce) => Ok(*nonce),
            None => Ok(0),
        }
    }

    /// Save a payment if it is valid against the ledger and the pool.
    pub fn add(&mut self, tx: Tx, state: &State) -> Result<(), String> {
        let mut preview = state.clone();
        for existing in &self.txs {
            tx::apply_payment(&mut preview, existing)?;
        }
        tx::apply_payment(&mut preview, &tx)?;
        self.txs.push(tx);
        self.save()
    }

    /// Remove payments that the new ledger tip already spent or rejected.
    pub fn reconcile(&mut self, state: &State) -> Result<(), String> {
        let mut preview = state.clone();
        let mut kept = Vec::new();
        let old_len = self.txs.len();

        for tx in self.txs.drain(..) {
            let mut trial = preview.clone();
            if tx::apply_payment(&mut trial, &tx).is_ok() {
                preview = trial;
                kept.push(tx);
            }
        }

        self.txs = kept;
        if self.txs.len() != old_len {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Err(error.to_string());
            }
        }
        let text = match serde_json::to_string_pretty(&self.txs) {
            Ok(text) => text,
            Err(error) => return Err(error.to_string()),
        };
        match fs::write(&self.path, text + "\n") {
            Ok(()) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
