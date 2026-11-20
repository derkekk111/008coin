//! Commands:
//!   coin new <name>
//!   coin address <name>
//!   coin balance <name>
//!   coin send <from> <to> <amount>
//!   coin pending
//!   coin mine <name>
//!   coin chain

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use coin::ledger::Ledger;
use coin::mempool::Mempool;
use coin::miner;
use coin::wallet::Wallets;

struct App {
    ledger: Ledger,
    mempool: Mempool,
    wallets: Wallets,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = env::args().skip(1).collect();

    // Optional: coin --datadir some/path <command>
    let dir = if args.first().map(|s| s.as_str()) == Some("--datadir") {
        args.remove(0);
        if args.is_empty() {
            return Err("missing datadir path".into());
        }
        PathBuf::from(args.remove(0))
    } else {
        PathBuf::from("data")
    };

    let cmd = if args.is_empty() {
        ""
    } else {
        args[0].as_str()
    };
    if cmd.is_empty() || cmd == "help" || cmd == "--help" {
        print_help();
        return Ok(());
    }

    if let Err(error) = fs::create_dir_all(&dir) {
        return Err(error.to_string());
    }
    let mut app = App::open(&dir)?;

    if cmd == "new" {
        let name = arg(&args, 1, "name")?;
        let wallet = app.wallets.create(&name)?;
        println!("name: {}", wallet.name);
        println!("address: {}", wallet.public);
    } else if cmd == "address" {
        let name = arg(&args, 1, "name")?;
        let wallet = app.wallets.get(&name)?;
        println!("{}", wallet.public);
    } else if cmd == "balance" {
        let who = arg(&args, 1, "name")?;
        let public_key = app.wallets.resolve(&who)?;
        println!("{}", app.ledger.balance(&public_key));
    } else if cmd == "send" {
        let from = arg(&args, 1, "from")?;
        let to = arg(&args, 2, "to")?;
        let amount_text = arg(&args, 3, "amount")?;
        let amount: u64 = match amount_text.parse() {
            Ok(amount) => amount,
            Err(_) => return Err("amount must be a positive integer".into()),
        };

        let from_wallet = app.wallets.get(&from)?.clone();
        let to_public = app.wallets.resolve(&to)?;
        let nonce = app
            .mempool
            .next_nonce(app.ledger.state(), &from_wallet.public)?;
        let tx = from_wallet.sign_payment(&to_public, amount, nonce)?;
        let id = tx.id();
        app.mempool.add(tx, app.ledger.state())?;
        println!("txid: {id}");
        println!("status: mempool");
    } else if cmd == "mine" {
        let name = arg(&args, 1, "name")?;
        let public_key = app.wallets.get(&name)?.public.clone();
        let (hash, height) = miner::seal(&mut app.ledger, &mut app.mempool, &public_key)?;
        println!("mined: {hash}");
        println!("height: {height}");
        println!("reward: {}", coin::REWARD);
    } else if cmd == "pending" {
        if app.mempool.is_empty() {
            println!("pending: 0");
        }
        for tx in app.mempool.txs() {
            let from = app.wallets.label(&tx.from);
            let to = app.wallets.label(&tx.to);
            println!(
                "{from} -> {to} amount {} nonce {} txid {}",
                tx.amount,
                tx.nonce,
                tx.id()
            );
        }
    } else if cmd == "chain" {
        print_chain(&app);
    } else {
        print_help();
        return Err(format!("unknown command {cmd}"));
    }
    Ok(())
}

impl App {
    fn open(dir: &PathBuf) -> Result<App, String> {
        let ledger = Ledger::open(&dir.join("ledger.jsonl"))?;
        let mut mempool = Mempool::open(&dir.join("mempool.json"))?;
        mempool.reconcile(ledger.state())?;
        let wallets = Wallets::open(&dir.join("wallets.json"))?;
        Ok(App {
            ledger,
            mempool,
            wallets,
        })
    }
}

fn print_chain(app: &App) {
    let count = app.ledger.blocks().len();
    println!("blocks: {count}");
    if count == 0 {
        println!("height: -");
        println!("tip: -");
        return;
    }

    let height = count - 1;
    let tip = app.ledger.tip().unwrap();
    println!("height: {height}");
    println!("tip: {tip}");

    let mut i = 0;
    for block in app.ledger.blocks() {
        let hash = coin::ledger::block_hash(block);
        println!(
            "[{i}] {hash} txs={} nonce={} time={}",
            block.txs.len(),
            block.header.nonce,
            block.header.time
        );
        i += 1;
    }
}

fn arg(args: &[String], index: usize, name: &str) -> Result<String, String> {
    if index >= args.len() {
        return Err(format!("missing {name}"));
    }
    Ok(args[index].clone())
}

fn print_help() {
    println!("coin new <name>                 create a key");
    println!("coin address <name>             print the public key");
    println!("coin balance <name>             confirmed balance");
    println!("coin send <from> <to> <amount>  sign a payment into the mempool");
    println!("coin pending                    list payments waiting to be mined");
    println!("coin mine <name>                seal a block, reward pays this wallet");
    println!("coin chain                      print the ledger");
    println!();
    println!("--datadir <path> may come before the command. Default directory is ./data.");
    println!("<to> may be a wallet name or a public key.");
}
