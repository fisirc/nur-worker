use aws_sdk_s3 as s3;

#[::tokio::main]
async fn main() {
    let config = aws_config::load_from_env().await;
    let client = aws_sdk_s3::Client::new(&config);
}
