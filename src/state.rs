use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusHandle;
use opendal::Operator;
use redis::aio::MultiplexedConnection;

use crate::handlers::kafka::KafkaPublisher;

/// Application state shared across all handlers
/// Uses dependency injection pattern - accepts pre-configured dependencies
#[derive(Clone)]
pub struct AppState {
    /// Redis multiplexed connection - supports Clone and handles concurrency internally
    /// No need for Arc<RwLock<>> - MultiplexedConnection is already designed for this
    pub redis: MultiplexedConnection,
    /// S3-compatible storage operator (RustFS/S3 via OpenDAL)
    pub storage: Operator,
    /// Kafka publisher for event publishing
    pub kafka_publisher: Option<Arc<KafkaPublisher>>,
    /// PostgreSQL connection pool (sqlx PgPool)
    pub postgres: Option<Arc<sqlx::PgPool>>,
    /// Prometheus metrics handle for /metrics endpoint
    pub metrics_handle: PrometheusHandle,
}

impl AppState {
    /// Create new AppState with dependency injection
    ///
    /// # Example
    /// ```compile_fail
    /// use redis::Client;
    /// use marrakesh::state::AppState;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = Client::open("redis://localhost:6379")?;
    ///     let conn = client.get_multiplexed_async_connection().await?;
    ///     let state = AppState::new(conn, storage, kafka_publisher, pool, metrics_handle);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(
        redis: MultiplexedConnection,
        storage: Operator,
        kafka_publisher: Option<Arc<KafkaPublisher>>,
        pool: Option<sqlx::PgPool>,
        metrics_handle: PrometheusHandle,
    ) -> Self {
        Self {
            redis,
            storage,
            kafka_publisher,
            postgres: pool.map(Arc::new),
            metrics_handle,
        }
    }
}
