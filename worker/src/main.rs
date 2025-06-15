use std::{env, error::Error};

mod logger;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    logger::build_logger().init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:6969".to_string());

    log::info!("⚒️ Starting Nur worker on {addr}");

    let server = server::Server::new(addr).await?;
    server.listen_forever_and_ever_amen().await?;

    Ok(())
}
