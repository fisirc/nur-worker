use std::{collections::HashMap, sync::Arc};
use tokio::{io::AsyncReadExt, sync::RwLock};
use uuid::Uuid;

pub struct CachedFunction {
    wasm_bytes: Arc<[u8]>,
    cached_at: u64,
}

#[derive(Clone)]
pub struct FunctionFetcher {
    s3_client: aws_sdk_s3::Client,
    memory_cache: Arc<RwLock<HashMap<Uuid, CachedFunction>>>,
    cache_dir: String,
}

#[derive(Debug)]
pub enum FetchFunctionError {
    Download,
    Decompression,
}

pub trait FunctionFetch {
    async fn fetch(
        &self,
        function_uuid: impl AsRef<Uuid>,
        last_deployment_timestamp: u64,
    ) -> Result<Arc<[u8]>, FetchFunctionError>;
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

        let cache_dir = crate::env::CACHE_DIR.clone();

        // Ensure the cache directory exists
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            return Err(format!("Failed to create cache directory: {e}"));
        }

        let s3_region: &'static str = crate::env::S3_REGION.clone().leak();

        let config = aws_config::defaults(aws_config::BehaviorVersion::v2025_01_17())
            .region(s3_region)
            .credentials_provider(credentials)
            .load()
            .await;

        let client = aws_sdk_s3::Client::new(&config);

        let memory_cache = Arc::new(RwLock::new(HashMap::new()));

        Ok(FunctionFetcher {
            s3_client: client,
            memory_cache,
            cache_dir,
        })
    }
}

impl FunctionFetch for FunctionFetcher {
    /// Returns the bytes of the WASM function uuid to run.
    /// Because we fetch the functions from the network, a cache mechanism is applied.
    /// The [last_deployment_timestamp] helps for cache invalidation: When the cached function
    /// has been stored before this deployment date, then need to fetch the newest version.
    async fn fetch(
        &self,
        function_uuid: impl AsRef<Uuid>,
        last_deployment_timestamp: u64,
    ) -> Result<Arc<[u8]>, FetchFunctionError> {
        let function_uuid = function_uuid.as_ref();
        let filename = format!("{cache}/{function_uuid}.wasm", cache = self.cache_dir);

        // L1 cache: Check if the function is in memory cache
        {
            let memory_cache = self.memory_cache.read().await;
            if let Some(cached_func) = memory_cache.get(function_uuid) {
                if cached_func.cached_at >= last_deployment_timestamp {
                    // If the cached function is newer than the last deployment timestamp, use it
                    log::debug!("Using in-memory cache for function {function_uuid}");
                    return Ok(cached_func.wasm_bytes.clone());
                }

                log::debug!("In-memory cache is outdated for function={function_uuid}");
                // Note: Instead of removing it here, we overwrite it later
            }
        }

        // L2 cache: Lets see if we can use or local filesystem cache for this
        let use_local_cache;

        match tokio::fs::metadata(&filename).await {
            Ok(metadata) => {
                // If the file is new enough, we use the cached value
                if let Ok(access_unix) = metadata.accessed() {
                    let cached_timestamp = access_unix
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    println!("--->{cached_timestamp}");

                    if last_deployment_timestamp > cached_timestamp {
                        log::debug!("Cached file {filename} is outdated. Fetching from network.");
                        use_local_cache = false;
                    } else {
                        use_local_cache = true;
                    }
                } else {
                    log::debug!("Could not get access time for {filename}. Fetching from network.");
                    use_local_cache = true;
                }
            }
            Err(_) => {
                // File does not exist, so must fetch from network
                log::debug!("Function {function_uuid} not found in cache. Fetching from network.");
                use_local_cache = true;
            }
        };

        if use_local_cache {
            log::debug!("Using local L2 cache: {filename}");
            let cached_file = tokio::fs::OpenOptions::new()
                .create(false)
                .read(true)
                .write(false)
                .open(&filename)
                .await;

            if cached_file.is_ok() {
                let mut cached_file_bytes = Vec::with_capacity(128);
                let mut cached_file = cached_file.unwrap();

                match cached_file.read_to_end(&mut cached_file_bytes).await {
                    Ok(_) => {
                        log::debug!("Using local L2 cache for {function_uuid}: {filename}");
                        // Store in memory cache
                        {
                            let mut memory_cache = self.memory_cache.write().await;
                            memory_cache.insert(
                                *function_uuid,
                                CachedFunction {
                                    wasm_bytes: Arc::from(cached_file_bytes.clone()),
                                    cached_at: current_unix_timestamp_s(),
                                },
                            );
                        }
                        return Ok(Arc::from(cached_file_bytes));
                    }
                    Err(e) => {
                        log::error!(
                            "Error reading {filename} from cache. Fallback to network: {e}"
                        );
                    }
                }
            }
        }

        let remote_filename = format!("builds/{function_uuid}.wasm.zst");

        log::debug!("Fetching from S3: {remote_filename}");
        let get_result = match self
            .s3_client
            .get_object()
            .bucket("nur-storage")
            .key(remote_filename)
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
        let wasm_bytes = Arc::from(wasm_bytes);

        // Save to cache
        if let Err(e) = tokio::fs::write(&filename, &wasm_bytes).await {
            log::error!("Failed to write wasm module to cache: {e}");
        }

        Ok(Arc::from(wasm_bytes))
    }
}

impl FunctionFetch for &'_ FunctionFetcher {
    async fn fetch(
        &self,
        function_uuid: impl AsRef<Uuid>,
        last_deployment_timestamp: u64,
    ) -> Result<Arc<[u8]>, FetchFunctionError> {
        (*self)
            .fetch(function_uuid, last_deployment_timestamp)
            .await
    }
}

fn current_unix_timestamp_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
