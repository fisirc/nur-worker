use std::{collections::HashMap, sync::Arc};
use tokio::{io::AsyncReadExt, sync::RwLock};
use uuid::Uuid;

/// Represents a fetched WASM module by a FunctionFetcher.
/// When [is_precompiled] is false, there is no guarantee that [wasm_bytes] is valid WASM.
///
/// Thanks to module-level type security, we can safely assume that
/// the [wasm_bytes] are valid, precompiled, WASM bytes when [is_precompiled] is true.
/// This is because only the [FunctionFetcher] can mark a module as precompiled,
#[derive(Clone, Debug)]
pub struct FetchedFunction {
    pub wasm_bytes: Arc<[u8]>,
    pub is_precompiled: bool,
    pub fetched_at: u64,
    _private: (),
}

#[derive(Clone)]
pub struct FunctionFetcher {
    s3_client: aws_sdk_s3::Client,
    memory_cache: Arc<RwLock<HashMap<Uuid, FetchedFunction>>>,
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
    ) -> Result<FetchedFunction, FetchFunctionError>;
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
    ) -> Result<FetchedFunction, FetchFunctionError> {
        let function_uuid = function_uuid.as_ref();

        // Assumptions:
        // L1 cache may be precompiled
        // L2 is ALWAYS precompiled
        // S3 storage is NEVER precompiled

        // L1 cache: Check if the function is in memory cache
        {
            let memory_cache = self.memory_cache.read().await;
            if let Some(cached_func) = memory_cache.get(function_uuid) {
                if cached_func.fetched_at >= last_deployment_timestamp {
                    // If the cached function is newer than the last deployment timestamp, use it
                    log::debug!("Using L1 in-memory cache for function {function_uuid}");
                    return Ok(cached_func.clone());
                }

                log::debug!("L1 In-memory cache is outdated for function={function_uuid}");
                // Note: Instead of removing it here, we overwrite it later
            }
        }

        // L2 cache: Lets see if we can use or local filesystem cache for this
        let mut use_local_cache = false;
        let filename = format!("{cache}/{function_uuid}.wasm.bin", cache = self.cache_dir);

        match tokio::fs::metadata(&filename).await {
            Ok(metadata) => {
                // If the file is new enough, we use the cached value
                if let Ok(access_unix) = metadata.modified() {
                    let cached_timestamp = access_unix
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    if last_deployment_timestamp < cached_timestamp {
                        use_local_cache = true;
                    } else {
                        log::debug!("Cached file {filename} is outdated. Fetching from network.");
                    }
                } else {
                    log::debug!("Could not get access time for {filename}. Fetching from network.");
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
                        let precompiled_func =
                            FetchedFunction::from_precompiled_wasm(Arc::from(cached_file_bytes));

                        // Store in memory cache
                        {
                            let mut memory_cache = self.memory_cache.write().await;
                            memory_cache.insert(*function_uuid, precompiled_func.clone());
                        }
                        return Ok(precompiled_func);
                    }
                    Err(e) => {
                        log::error!(
                            "Error reading {filename} from cache. Fallback to network: {e}"
                        );
                    }
                }
            } else {
                log::debug!(
                    "Cached file {filename} does not exist or is not readable. Fetching from network."
                );
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
        let precompiled_func = FetchedFunction::try_precompile(Arc::from(wasm_bytes));

        // Save to L2 cache
        if let Err(e) = tokio::fs::write(&filename, &precompiled_func.wasm_bytes).await {
            log::error!("Failed to write wasm module to cache: {e}");
        }

        Ok(precompiled_func)
    }
}

impl FunctionFetch for &'_ FunctionFetcher {
    async fn fetch(
        &self,
        function_uuid: impl AsRef<Uuid>,
        last_deployment_timestamp: u64,
    ) -> Result<FetchedFunction, FetchFunctionError> {
        (*self)
            .fetch(function_uuid, last_deployment_timestamp)
            .await
    }
}

impl FetchedFunction {
    // Private, because no external code should mark arbitrary WASM modules as precompiled.
    fn from_precompiled_wasm(wasm_bytes: Arc<[u8]>) -> Self {
        FetchedFunction {
            wasm_bytes,
            is_precompiled: true,
            fetched_at: current_unix_timestamp_s(),
            _private: (),
        }
    }

    pub fn from_wasm(wasm_bytes: Arc<[u8]>) -> Self {
        FetchedFunction {
            wasm_bytes,
            is_precompiled: false,
            fetched_at: current_unix_timestamp_s(),
            _private: (),
        }
    }

    pub fn try_precompile(wasm_bytes: Arc<[u8]>) -> Self {
        if let Some(precompiled_bytes) = precompile_wasm_bytes(&wasm_bytes) {
            FetchedFunction::from_precompiled_wasm(Arc::from(precompiled_bytes))
        } else {
            FetchedFunction::from_wasm(wasm_bytes)
        }
    }
}

fn current_unix_timestamp_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn precompile_wasm_bytes(wasm_bytes: &Arc<[u8]>) -> Option<Vec<u8>> {
    let store = wasmer::Store::default();
    let module = wasmer::Module::new(&store, wasm_bytes).ok()?;
    let bytes = module.serialize().ok()?;
    Some(bytes.to_vec())
}
