use marrakesh::observability;
use marrakesh::state::AppState;

use opendal::Operator;

/// Create a test AppState with a mock Redis connection
/// For integration tests, use a real Redis instance (e.g., via docker-compose or testcontainers)
pub async fn create_test_state() -> AppState {
    let redis_url =
        std::env::var("APP__REDIS__URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let client = redis::Client::open(redis_url).expect("Failed to create Redis client");
    let conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect to Redis");

    // Initialize metrics properly (same as production) to ensure tests match runtime behavior
    let metrics_handle = observability::init_metrics().expect("Failed to initialize metrics");
    let storage = create_test_storage();
    AppState::new(conn, storage, None, None, metrics_handle)
}

/// Create a test storage operator pointing to local RustFS
fn create_test_storage() -> Operator {
    let builder = opendal::services::S3::default()
        .endpoint("http://localhost:9000")
        .bucket("data")
        .region("us-east-1");

    Operator::new(builder).unwrap().finish()
}
