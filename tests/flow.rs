use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use coin::ledger::Ledger;
use coin::mempool::Mempool;
use coin::miner;
use coin::tx;
use coin::wallet::Wallets;
use coin::{GENESIS_PREV, REWARD};

fn tmp() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("coin-{nanos}-{n}"));
    fs::create_dir_all(&path).unwrap();
    path
}

struct Node {
    dir: PathBuf,
    ledger: Ledger,
    mempool: Mempool,
    wallets: Wallets,
}

impl Node {
    fn open(dir: PathBuf) -> Self {
        let ledger = Ledger::open(&dir.join("ledger.jsonl")).unwrap();
        let mut mempool = Mempool::open(&dir.join("mempool.json")).unwrap();
        mempool.reconcile(ledger.state()).unwrap();
        let wallets = Wallets::open(&dir.join("wallets.json")).unwrap();
        Self {
            dir,
            ledger,
            mempool,
            wallets,
        }
    }

    fn fresh() -> Self {
        Self::open(tmp())
    }
}

#[test]
fn genesis_prev_is_32_bytes() {
    assert_eq!(GENESIS_PREV.len(), 64);
}

#[test]
fn mine_send_and_mine_again() {
    let mut node = Node::fresh();
    let alice = node.wallets.create("alice").unwrap().public.clone();
    let bob = node.wallets.create("bob").unwrap().public.clone();

    let (hash, height) = miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    assert_eq!(height, 0);
    assert!(hash.starts_with("0000"), "{hash}");
    assert_eq!(node.ledger.balance(&alice), REWARD);
    assert_eq!(node.ledger.blocks()[0].header.prev, GENESIS_PREV);

    let alice_wallet = node.wallets.get("alice").unwrap().clone();
    let nonce = node
        .mempool
        .next_nonce(node.ledger.state(), &alice)
        .unwrap();
    let tx = alice_wallet.sign_payment(&bob, 10, nonce).unwrap();
    node.mempool.add(tx, node.ledger.state()).unwrap();
    assert_eq!(node.ledger.balance(&alice), REWARD);
    assert_eq!(node.mempool.len(), 1);

    let again = alice_wallet.sign_payment(&bob, REWARD, 0).unwrap();
    let err = node.mempool.add(again, node.ledger.state()).unwrap_err();
    assert!(
        err.contains("nonce") || err.contains("insufficient"),
        "{err}"
    );

    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    assert!(node.mempool.is_empty());
    assert_eq!(node.ledger.balance(&bob), 10);
    assert_eq!(node.ledger.balance(&alice), REWARD - 10 + REWARD);

    let reloaded = Node::open(node.dir.clone());
    assert_eq!(reloaded.ledger.balance(&alice), node.ledger.balance(&alice));
    assert_eq!(reloaded.ledger.balance(&bob), 10);
    assert_eq!(reloaded.ledger.tip(), node.ledger.tip());
}

#[test]
fn changed_payment_is_rejected() {
    let mut node = Node::fresh();
    let alice = node.wallets.create("alice").unwrap().public.clone();
    let bob = node.wallets.create("bob").unwrap().public.clone();
    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    let wallet = node.wallets.get("alice").unwrap().clone();
    let mut tx = wallet.sign_payment(&bob, 10, 0).unwrap();
    tx.amount = 11;
    let err = node.mempool.add(tx, node.ledger.state()).unwrap_err();
    assert_eq!(err, "bad signature");
}

#[test]
fn bad_block_does_not_move_the_tip() {
    let mut node = Node::fresh();
    let alice = node.wallets.create("alice").unwrap().public.clone();
    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    let tip = node.ledger.tip().unwrap();
    let mut block = node.ledger.blocks()[0].clone();
    block.header.nonce = block.header.nonce.wrapping_add(1);
    let err = node.ledger.append(block).unwrap_err();
    assert!(
        err.contains("proof of work") || err.contains("tip"),
        "{err}"
    );
    assert_eq!(node.ledger.tip().as_deref(), Some(tip.as_str()));
}

#[test]
fn edited_ledger_file_is_rejected() {
    let mut node = Node::fresh();
    let alice = node.wallets.create("alice").unwrap().public.clone();
    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    let path = node.dir.join("ledger.jsonl");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("\"amount\":50", "\"amount\":51");
    fs::write(&path, text).unwrap();
    match Ledger::open(&path) {
        Ok(_) => panic!("edited ledger was accepted"),
        Err(err) => {
            assert!(
                err.contains("reward") || err.contains("mismatch") || err.contains("proof"),
                "{err}"
            );
        }
    }
}

#[test]
fn two_sequential_payments_mine_together() {
    let mut node = Node::fresh();
    let alice = node.wallets.create("alice").unwrap().public.clone();
    let bob = node.wallets.create("bob").unwrap().public.clone();
    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    let wallet = node.wallets.get("alice").unwrap().clone();
    let first = wallet.sign_payment(&bob, 15, 0).unwrap();
    let second = wallet.sign_payment(&bob, 20, 1).unwrap();
    node.mempool.add(first, node.ledger.state()).unwrap();
    node.mempool.add(second, node.ledger.state()).unwrap();
    miner::seal(&mut node.ledger, &mut node.mempool, &alice).unwrap();
    assert_eq!(node.ledger.balance(&bob), 35);
    assert_eq!(node.ledger.balance(&alice), REWARD - 35 + REWARD);
    assert!(node.mempool.is_empty());
}

#[test]
fn reward_transaction_is_not_a_signed_payment() {
    let mut state = tx::State::default();
    let reward = tx::Tx::reward(&coin::crypto::generate_keypair().1, 0);
    tx::apply_reward(&mut state, &reward, 0).unwrap();
    assert_eq!(state.balances[&reward.to], REWARD);
    assert!(tx::apply_payment(&mut state, &reward).is_err());
}

#[test]
fn cli_story() {
    let dir = tmp();
    let bin = env!("CARGO_BIN_EXE_coin");
    let run = |args: &[&str]| {
        let output = std::process::Command::new(bin)
            .arg("--datadir")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            output.status.success(),
            "command {args:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        stdout
    };

    let created = run(&["new", "alice"]);
    assert!(created.contains("address: "));
    run(&["new", "bob"]);
    let mined = run(&["mine", "alice"]);
    assert!(mined.contains("height: 0"));
    assert_eq!(run(&["balance", "alice"]).trim(), "50");
    let sent = run(&["send", "alice", "bob", "10"]);
    assert!(sent.contains("status: mempool"));
    assert_eq!(run(&["balance", "alice"]).trim(), "50");
    assert_eq!(run(&["balance", "bob"]).trim(), "0");
    run(&["mine", "alice"]);
    assert_eq!(run(&["balance", "bob"]).trim(), "10");
    assert_eq!(run(&["balance", "alice"]).trim(), "90");
    let chain = run(&["chain"]);
    assert!(chain.contains("blocks: 2"));
    assert!(chain.contains("height: 1"));

    let over = std::process::Command::new(bin)
        .arg("--datadir")
        .arg(&dir)
        .args(["send", "bob", "alice", "11"])
        .output()
        .unwrap();
    assert!(!over.status.success());
    let err = String::from_utf8(over.stderr).unwrap();
    assert!(err.contains("insufficient"), "{err}");
}
