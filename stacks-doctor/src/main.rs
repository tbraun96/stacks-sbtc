use clap::Parser;
use cli::Commands;
use commands::{analyze_logs, burns, show_env};

use crate::cli::Args;

mod cli;
mod commands;

fn main() {
    let args = Args::parse();

    match &args.cmd {
        Commands::Analyze => {
            if let Some(log_file) = args.log_file {
                analyze_logs(log_file)
            } else {
                eprintln!("Log file path needs to be passed");
                Ok(())
            }
        }
        Commands::Burns(burns_args) => {
            if let Some(db_dir) = args.db_dir {
                burns(args.network, &db_dir, burns_args)
            } else {
                eprintln!("Database directory path needs to be passed");
                Ok(())
            }
        }
        Commands::Env => show_env(),
    }
    .unwrap();
}
