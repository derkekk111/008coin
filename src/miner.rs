//! The miner.
//!
//! It puts the 50-coin reward first, then any waiting payments.
//! Then it tries nonce = 0, 1, 2, ... until the block hash starts with "0000".

use crate::crypto;
use crate::ledger::{self, Block, Header, Ledger};
use crate::mempool::Mempool;
use crate::tx::{self, Tx};
use crate::GENESIS_PREV;

/// Mine one block onto the ledger. The reward pays `reward_to`.
/// Returns (block_hash, height).
pub fn seal(
    ledger: &mut Ledger,
    mempool: &mut Mempool,
    reward_to: &str,
) -> Result<(String, u64), String> {
    if !crypto::is_pubkey(reward_to) {
        return Err("invalid public key".into());
    }

    let height = ledger.next_height();
    let mut state = ledger.state().clone();

    // First payment in the block creates 50 coins.
    let reward = Tx::reward(reward_to, height);
    tx::apply_reward(&mut state, &reward, height)?;

    let mut txs = vec![reward];
    for tx in mempool.txs() {
        let mut trial = state.clone();
        if tx::apply_payment(&mut trial, tx).is_ok() {
            state = trial;
            txs.push(tx.clone());
        }
    }

    let prev = match ledger.tip() {
        Some(hash) => hash,
        None => GENESIS_PREV.to_string(),
    };
    let parent_time = match ledger.tip_time() {
        Some(time) => time,
        None => 0,
    };
    let mut time = ledger::now();
    if time < parent_time {
        time = parent_time;
    }

    let block = mine_block(&prev, txs, time);
    let hash = ledger.append(block)?;
    mempool.reconcile(ledger.state())?;
    Ok((hash, height))
}

/// Try nonces until the header hash is under the target.
pub fn mine_block(prev: &str, txs: Vec<Tx>, time: u64) -> Block {
    let tx_hash = tx::tx_root(&txs);
    let mut nonce: u64 = 0;

    loop {
        let header = Header {
            prev: prev.to_string(),
            tx_hash: tx_hash.clone(),
            time,
            nonce,
        };
        let hash = ledger::header_hash(&header);
        if crypto::under_target(&hash) {
            return Block { header, txs };
        }
        nonce += 1;
    }
}
