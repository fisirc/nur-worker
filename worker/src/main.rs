use crate::fetcher::FunctionFetcher;
use std::error::Error;

mod env;
mod fetcher;
mod handshake;
mod intrinsics;
mod logger;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();
    logger::build_logger().init();

    let host = env::HOST.clone();
    let port = *env::PORT;

    log::info!("⌛️ Starting Nur worker...");

    let function_fetcher = FunctionFetcher::from_env().await?;

    let server = server::Server::new((host.clone(), port), function_fetcher).await?;

    log::info!("⚒️ Ready to listen at {host}:{port}");

    server.listen_forever_and_ever_amen().await?;

    Ok(())
}
