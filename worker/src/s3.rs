use aws_sdk_s3 as s3;

async fn load() {
    let config = aws_config::load_defaults(BehaviorVersion::v2025_01_17()).await;
    let client = s3::Client::new(&config);
}
