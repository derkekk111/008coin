//! Payments.
//!
//! A normal payment moves coins from one public key to another.
//! A reward payment creates 50 new coins for the miner. It is not signed.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::crypto::{self, sha256_hex};
use crate::REWARD;

/// One payment.
///
/// `from` and `to` are public keys, except a reward, where `from` is "coinbase".
/// `nonce` is 0 for an account's first payment, then 1, then 2, and so on.
/// For a reward, `nonce` is the block height instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tx {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub nonce: u64,
    pub signature: String,
}

impl Tx {
    /// The unsigned reward at the start of a block.
    pub fn reward(to: &str, height: u64) -> Tx {
        Tx {
            from: "coinbase".to_string(),
            to: to.to_string(),
            amount: REWARD,
            nonce: height,
            signature: String::new(),
        }
    }

    /// The text the sender signs. The signature is not part of this text.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let text = format!(
            "from={}\nto={}\namount={}\nnonce={}",
            self.from, self.to, self.amount, self.nonce
        );
        text.into_bytes()
    }

    /// The text stored in a block. This includes the signature.
    pub fn committed_bytes(&self) -> Vec<u8> {
        let signed = String::from_utf8(self.signing_bytes()).expect("payment text is ascii");
        let text = format!("{signed}\nsignature={}", self.signature);
        text.into_bytes()
    }

    /// Id of this payment. It changes if the amount, nonce, or signature changes.
    pub fn id(&self) -> String {
        sha256_hex(&self.committed_bytes())
    }
}

/// One hash that covers every payment in a block, in order.
pub fn tx_root(txs: &[Tx]) -> String {
    let mut body = String::new();
    for tx in txs {
        body.push_str(&tx.id());
        body.push('\n');
    }
    sha256_hex(body.as_bytes())
}

/// Balances and the next nonce for each account.
/// This is rebuilt by replaying the ledger. It is not saved by itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub balances: HashMap<String, u64>,
    pub nonces: HashMap<String, u64>,
}

/// Add the block reward to the miner's balance.
pub fn apply_reward(state: &mut State, tx: &Tx, height: u64) -> Result<(), String> {
    if tx.from != "coinbase" || !tx.signature.is_empty() {
        return Err("malformed reward".into());
    }
    if tx.amount != REWARD {
        return Err("bad reward".into());
    }
    if tx.nonce != height {
        return Err("reward height mismatch".into());
    }
    if !crypto::is_pubkey(&tx.to) {
        return Err("invalid public key".into());
    }
    add_coins(state, &tx.to, REWARD)
}

/// Check a normal payment and update balances if it is valid.
pub fn apply_payment(state: &mut State, tx: &Tx) -> Result<(), String> {
    if tx.from == "coinbase" {
        return Err("malformed transaction".into());
    }
    if !crypto::is_pubkey(&tx.from) || !crypto::is_pubkey(&tx.to) {
        return Err("invalid public key".into());
    }
    if tx.amount == 0 {
        return Err("invalid amount".into());
    }
    if !crypto::verify(&tx.from, &tx.signing_bytes(), &tx.signature) {
        return Err("bad signature".into());
    }

    let expected = match state.nonces.get(&tx.from) {
        Some(nonce) => *nonce,
        None => 0,
    };
    if tx.nonce != expected {
        return Err(format!("bad nonce: expected {expected}"));
    }

    let balance = match state.balances.get(&tx.from) {
        Some(amount) => *amount,
        None => 0,
    };
    if balance < tx.amount {
        return Err(format!(
            "insufficient funds: have {balance} need {}",
            tx.amount
        ));
    }

    // Paying yourself only uses up the nonce. The balance stays the same.
    if tx.from != tx.to {
        subtract_coins(state, &tx.from, tx.amount)?;
        add_coins(state, &tx.to, tx.amount)?;
    }
    state.nonces.insert(tx.from.clone(), expected + 1);
    Ok(())
}

fn add_coins(state: &mut State, account: &str, amount: u64) -> Result<(), String> {
    let old = match state.balances.get(account) {
        Some(amount) => *amount,
        None => 0,
    };
    let new_amount = match old.checked_add(amount) {
        Some(sum) => sum,
        None => return Err("balance overflow".into()),
    };
    state.balances.insert(account.to_string(), new_amount);
    Ok(())
}

fn subtract_coins(state: &mut State, account: &str, amount: u64) -> Result<(), String> {
    let old = match state.balances.get(account) {
        Some(amount) => *amount,
        None => 0,
    };
    if old < amount {
        return Err("insufficient funds".into());
    }
    state.balances.insert(account.to_string(), old - amount);
    Ok(())
}
