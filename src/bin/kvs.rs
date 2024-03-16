use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Set { key: String, value: String },
    Get { key: String },
    Rm { key: String },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Set { key: _, value: _ } => {
            eprintln!("unimplemented");
            process::exit(1);
        }
        Commands::Get { key: _ } => {
            eprintln!("unimplemented");
            process::exit(1);
        }
        Commands::Rm { key: _ } => {
            eprintln!("unimplemented");
            process::exit(1);
        }
    }
}
