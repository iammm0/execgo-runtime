use clap::Parser;
use execgo_runtime::{cli::Cli, runtime};

#[tokio::main]
async fn main() {
    if let Err(err) = runtime::run(Cli::parse()).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
