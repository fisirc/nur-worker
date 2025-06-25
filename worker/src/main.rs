use crate::{fetcher::FunctionFetcher, logs_service::SupabaseLogService};
use std::error::Error;

mod env;
mod fetcher;
mod handshake;
mod intrinsics;
mod logger;
mod logs_service;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();
    logger::build_logger().init();

    let server_addr = (env::HOST.clone(), *env::PORT);

    log::info!("⌛️ Starting Nur worker...");

    let logs_service = SupabaseLogService::from_env();

    logs_service.check_connection().await?;

    let function_fetcher = FunctionFetcher::from_env().await?;
    let server = server::Server::new(&server_addr, function_fetcher, logs_service).await?;

    log::info!("⚒️ Ready to listen at {server_addr:?}");

    server.listen_forever_and_ever_amen().await?;

    Ok(())
}
