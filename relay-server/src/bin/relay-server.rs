use clap::Parser;
use relay_server::Server;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Cli::parse();
    Server::run(&args.listen).await;
}

#[derive(Parser)]
struct Cli {
    /// Where to listen for incoming connections
    #[clap(short, long, default_value = "127.0.0.1:9776")]
    listen: String,
}
