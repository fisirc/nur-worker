use tokio::io::AsyncReadExt;

const CACHE_DIR: &str = ".cache";

#[derive(Clone)]
pub struct FunctionFetcher {
    s3_client: aws_sdk_s3::Client,
}

#[derive(Debug)]
pub enum FetchFunctionError {
    Download,
    Decompression,
}

impl FunctionFetcher {
    pub async fn from_env() -> Result<Self, String> {
        let credentials = aws_sdk_s3::config::Credentials::new(
            crate::env::S3_ACCESS_KEY_ID.clone(),
            crate::env::S3_SECRET_ACCESS_KEY.clone(),
            None,
            None,
            "nur",
        );

        let s3_region: &'static str = crate::env::S3_REGION.clone().leak();

        let config = aws_config::defaults(aws_config::BehaviorVersion::v2025_01_17())
            .region(s3_region)
            .credentials_provider(credentials)
            .load()
            .await;

        let client = aws_sdk_s3::Client::new(&config);

        Ok(FunctionFetcher { s3_client: client })
    }

    pub async fn fetch(&self, func_name: impl AsRef<str>) -> Result<Vec<u8>, FetchFunctionError> {
        let get_result = match self
            .s3_client
            .get_object()
            .bucket("nur-storage")
            .key(format!("{}.wasm.zst", func_name.as_ref()))
            .send()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                log::error!("Failed to fetch wasm module from S3: {e}");
                return Err(FetchFunctionError::Download);
            }
        };

        let wasm_zst_reader = get_result.body.into_async_read();
        let mut decompression =
            async_compression::tokio::bufread::ZstdDecoder::new(wasm_zst_reader);

        let mut wasm_bytes = Vec::<u8>::with_capacity(128);
        match decompression.read_to_end(&mut wasm_bytes).await {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to decompress wasm module from S3: {e}");
                return Err(FetchFunctionError::Decompression);
            }
        }

        Ok(wasm_bytes)
    }
}
