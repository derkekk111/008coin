//! The ledger is a file of blocks, one block per line.
//!
//! The file is only appended to. Balances come from reading it from the start.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::crypto::{self, sha256_hex};
use crate::tx::{self, State, Tx};
use crate::{GENESIS_PREV, MAX_FUTURE_SECS};

/// The part of a block that gets hashed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub prev: String,
    pub tx_hash: String,
    pub time: u64,
    pub nonce: u64,
}

/// A header plus the payments inside the block. The reward is txs[0].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub header: Header,
    pub txs: Vec<Tx>,
}

pub struct Ledger {
    path: PathBuf,
    blocks: Vec<Block>,
    state: State,
}

impl Ledger {
    /// Load the file. An empty or missing file means there are no blocks yet.
    pub fn open(path: &Path) -> Result<Ledger, String> {
        let mut ledger = Ledger {
            path: path.to_path_buf(),
            blocks: Vec::new(),
            state: State::default(),
        };
        if !path.exists() {
            return Ok(ledger);
        }

        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => return Err(error.to_string()),
        };
        if text.is_empty() {
            return Ok(ledger);
        }

        let mut line_number = 1;
        for line in text.lines() {
            if line.trim().is_empty() {
                return Err(format!("corrupt ledger at line {line_number}"));
            }
            let block = match serde_json::from_str::<Block>(line) {
                Ok(block) => block,
                Err(error) => {
                    return Err(format!("corrupt ledger at line {line_number}: {error}"));
                }
            };
            // persist = false: the block is already in the file.
            ledger.ingest(block, false, now())?;
            line_number += 1;
        }
        Ok(ledger)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Height the next block will have. The first block is height 0.
    pub fn next_height(&self) -> u64 {
        self.blocks.len() as u64
    }

    pub fn tip(&self) -> Option<String> {
        match self.blocks.last() {
            Some(block) => Some(block_hash(block)),
            None => None,
        }
    }

    pub fn tip_time(&self) -> Option<u64> {
        match self.blocks.last() {
            Some(block) => Some(block.header.time),
            None => None,
        }
    }

    pub fn balance(&self, public_key: &str) -> u64 {
        match self.state.balances.get(public_key) {
            Some(amount) => *amount,
            None => 0,
        }
    }

    /// Check a new block and append it to the file.
    pub fn append(&mut self, block: Block) -> Result<String, String> {
        self.ingest(block, true, now())
    }

    fn ingest(&mut self, block: Block, persist: bool, now_secs: u64) -> Result<String, String> {
        let parent_hash = self.tip();
        let parent_time = self.tip_time();
        let height = self.next_height();
        let state = validate(
            &block,
            parent_hash.as_deref(),
            parent_time,
            height,
            &self.state,
            now_secs,
        )?;
        let hash = block_hash(&block);
        if persist {
            self.save_block(&block)?;
        }
        self.blocks.push(block);
        self.state = state;
        Ok(hash)
    }

    fn save_block(&self, block: &Block) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Err(error.to_string());
            }
        }
        let line = match serde_json::to_string(block) {
            Ok(line) => line,
            Err(error) => return Err(error.to_string()),
        };
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) => return Err(error.to_string()),
        };
        if let Err(error) = writeln!(file, "{line}") {
            return Err(error.to_string());
        }
        if let Err(error) = file.flush() {
            return Err(error.to_string());
        }
        Ok(())
    }
}

pub fn header_hash(header: &Header) -> String {
    let text = format!(
        "prev={}\ntx_hash={}\ntime={}\nnonce={}",
        header.prev, header.tx_hash, header.time, header.nonce
    );
    sha256_hex(text.as_bytes())
}

pub fn block_hash(block: &Block) -> String {
    header_hash(&block.header)
}

/// Reject the block on the first broken rule.
fn validate(
    block: &Block,
    parent_hash: Option<&str>,
    parent_time: Option<u64>,
    height: u64,
    state: &State,
    now_secs: u64,
) -> Result<State, String> {
    let expected_prev = match parent_hash {
        Some(hash) => hash,
        None => GENESIS_PREV,
    };
    if block.header.prev != expected_prev {
        return Err("block does not extend the tip".into());
    }
    if block.txs.is_empty() {
        return Err("empty block".into());
    }
    if tx::tx_root(&block.txs) != block.header.tx_hash {
        return Err("transaction hash mismatch".into());
    }
    if !crypto::under_target(&block_hash(block)) {
        return Err("proof of work is invalid".into());
    }
    if let Some(parent_time) = parent_time {
        if block.header.time < parent_time {
            return Err("block timestamp before parent".into());
        }
    }
    if block.header.time > now_secs + MAX_FUTURE_SECS {
        return Err("block timestamp too far in the future".into());
    }

    let mut next = state.clone();
    tx::apply_reward(&mut next, &block.txs[0], height)?;
    let mut i = 1;
    while i < block.txs.len() {
        tx::apply_payment(&mut next, &block.txs[i])?;
        i += 1;
    }
    Ok(next)
}

pub fn now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}
