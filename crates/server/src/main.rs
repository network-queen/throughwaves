use clap::Parser;
use jamhub_network::server::SessionServer;

#[derive(Parser)]
#[command(name = "jamhub-server", about = "JamHub collaborative session server")]
struct Args {
    /// Address to listen on
    #[arg(short, long, default_value = "0.0.0.0:9090")]
    addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("Starting JamHub server...");

    let server = SessionServer::new();
    server.run(&args.addr).await?;

    Ok(())
}
