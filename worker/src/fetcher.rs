use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub struct FunctionFetcher {
    s3_client: aws_sdk_s3::Client,
    cache_dir: String,
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

        let cache_dir = crate::env::CACHE_DIR.clone();

        // Ensure the cache directory exists
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            return Err(format!("Failed to create cache directory: {e}"));
        }

        Ok(FunctionFetcher {
            s3_client: client,
            cache_dir,
        })
    }

    /// Returns the bytes of the WASM function uuid to run.
    /// Because we fetch the functions from the network, a cache mechanism is applied.
    /// The [last_deployment_timestamp] helps for cache invalidation: When the cached function
    /// has been stored before this deployment date, then need to fetch the newest version.
    pub async fn fetch(
        &self,
        function_uuid: impl AsRef<str>,
        last_deployment_timestamp: u64,
    ) -> Result<Vec<u8>, FetchFunctionError> {
        let function_uuid = function_uuid.as_ref();
        let filename = format!("{cache}/{function_uuid}.wasm", cache = self.cache_dir);

        let mut fetch_from_cache = false;

        match tokio::fs::metadata(&filename).await {
            Ok(metadata) => {
                // If the file is new enough, we use the cached value
                if let Ok(access_unix) = metadata.accessed() {
                    let cached_timestamp = access_unix
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if last_deployment_timestamp > cached_timestamp {
                        fetch_from_cache = true;
                    }
                } else {
                    fetch_from_cache = true;
                }
            }
            Err(_) => {
                // File does not exist, so must fetch from network
                fetch_from_cache = true;
            }
        };

        if fetch_from_cache {
            let cached_file = tokio::fs::OpenOptions::new()
                .create(false)
                .read(true)
                .write(false)
                .open(&filename)
                .await;

            if cached_file.is_ok() {
                let mut cached_file_bytes = Vec::new();
                let mut cached_file = cached_file.unwrap();

                match cached_file.read_to_end(&mut cached_file_bytes).await {
                    Ok(_) => {
                        return Ok(cached_file_bytes);
                    }
                    Err(e) => {
                        log::error!(
                            "Error reading {filename} from cache. Fallback to network: {e}"
                        );
                    }
                }
            }
        }

        let get_result = match self
            .s3_client
            .get_object()
            .bucket("nur-storage")
            .key(format!("{}.wasm.zst", function_uuid))
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

        // Save to cache
        if let Err(e) = tokio::fs::write(&filename, &wasm_bytes).await {
            log::error!("Failed to write wasm module to cache: {e}");
        }

        return Ok(wasm_bytes);
    }
}
