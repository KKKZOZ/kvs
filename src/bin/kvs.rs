use std::{env::current_dir, process};

use clap::Parser;

use kvs::Commands;

#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() -> kvs::Result<()> {
    let cli = Cli::parse();

    let mut kvs = kvs::KvStore::open(current_dir()?).unwrap();

    match cli.command {
        Commands::Set { key, value } => {
            kvs.set(key, value)?;
            Ok(())
        }
        Commands::Get { key } => {
            let value = kvs.get(key)?;
            let value = value.unwrap_or_else(|| {
                println!("Key not found");
                process::exit(0);
            });
            println!("{}", value);
            Ok(())
        }
        Commands::Rm { key } => {
            let result = kvs.remove(key);
            if result.is_err() {
                println!("Key not found");
                process::exit(1);
            }
            Ok(())
        }
    }
}
