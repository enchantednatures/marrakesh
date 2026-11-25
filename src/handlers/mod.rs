pub mod api;
pub mod events;
// Health endpoints are infrastructure: they live in `service-core` and are
// re-exported here so `handlers::health::*` paths keep working.
pub use service_core::health;
pub mod items;
pub mod kafka;
pub mod storage;
pub mod validation;
