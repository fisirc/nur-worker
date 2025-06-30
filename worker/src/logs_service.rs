use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone)]
pub struct SupabaseLogService {
    client_pool: deadpool_postgres::Pool,
}

impl SupabaseLogService {
    pub fn from_env() -> Self {
        let pg_config = tokio_postgres::Config::from_str(&crate::env::POSTGRES_URL)
            .expect("Invalid POSTGRES_URL variable");

        let mgr_config = deadpool_postgres::ManagerConfig {
            recycling_method: deadpool_postgres::RecyclingMethod::Fast,
        };

        let mgr =
            deadpool_postgres::Manager::from_config(pg_config, tokio_postgres::NoTls, mgr_config);

        let client_pool = deadpool_postgres::Pool::builder(mgr)
            .max_size(16)
            .build()
            .unwrap();

        SupabaseLogService { client_pool }
    }

    pub async fn check_connection(&self) -> Result<(), deadpool_postgres::PoolError> {
        // Just to ensure the connection is established
        self.client_pool.get().await.map(|_| ())
    }
}

impl LogsService for SupabaseLogService {
    async fn send(&self, function_uuid: &Uuid, message: &str) -> Result<(), tokio_postgres::Error> {
        let client = self.client_pool.get().await.unwrap();
        let stmt = client
            .prepare_cached("INSERT INTO function_logs(function_id, message) VALUES ($1, $2)")
            .await?;

        client.execute(&stmt, &[&function_uuid, &message]).await?;
        log::trace!("send({function_uuid:?}, {message})");
        Ok(())
    }
}

pub trait LogsService {
    async fn send(&self, function_id: &Uuid, message: &str) -> Result<(), tokio_postgres::Error>;
}
