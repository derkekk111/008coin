//! A small coin for one computer.
//!
//! ledger  = the saved blocks and the balances
//! wallet  = keys that sign payments
//! mempool = payments waiting to be mined
//! miner   = finds a proof-of-work nonce and adds a block

pub mod crypto;
pub mod ledger;
pub mod mempool;
pub mod miner;
pub mod tx;
pub mod wallet;

/// Coins created in every block, paid to the miner.
pub const REWARD: u64 = 50;

/// The block hash must start with this many zero bits.
/// 16 bits means the hex hash starts with "0000".
pub const DIFFICULTY_BITS: u32 = 16;

/// A block timestamp may be at most two hours ahead of the clock.
pub const MAX_FUTURE_SECS: u64 = 2 * 60 * 60;

/// The first block's previous hash. 64 zeros.
pub const GENESIS_PREV: &str = "0000000000000000000000000000000000000000000000000000000000000000";
