mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_cloudevent_ping() {
    let state = common::create_test_state().await;
    let server = common::create_test_server(state);

    let response = server
        .post("/")
        .content_type("application/json")
        .json(&json!({
            "specversion": "1.0",
            "id": "test-123",
            "type": "ping",
            "source": "/test",
            "data": {
                "message": "hello"
            }
        }))
        .await;

    response.assert_status_ok();
    response.assert_json(&json!({
        "reply": "pong: hello",
        "event_id": "test-123"
    }));
}

#[tokio::test]
async fn test_cloudevent_structured_body_with_ce_headers() {
    // Regression for the Knative Kafka broker dispatcher shape: a full
    // structured CloudEvent body delivered with `ce-*` headers. The extractor
    // must prefer the structured body instead of reading it as binary data.
    let state = common::create_test_state().await;
    let server = common::create_test_server(state);

    let response = server
        .post("/")
        .add_header("ce-specversion", "1.0")
        .add_header("ce-id", "test-dispatcher")
        .add_header("ce-source", "/apis/v1/namespaces/default/kafkasources/test")
        .add_header("ce-type", "dev.knative.kafka.event")
        .content_type("application/json")
        .json(&json!({
            "specversion": "1.0",
            "id": "test-dispatcher",
            "type": "dev.knative.kafka.event",
            "source": "/apis/v1/namespaces/default/kafkasources/test",
            "data": {
                "message": "from dispatcher"
            }
        }))
        .await;

    response.assert_status_ok();
    response.assert_json(&json!({
        "reply": "pong: from dispatcher",
        "event_id": "test-dispatcher"
    }));
}

#[tokio::test]
async fn test_cloudevent_missing_id() {
    let state = common::create_test_state().await;
    let server = common::create_test_server(state);

    let response = server
        .post("/")
        .content_type("application/json")
        .json(&json!({
            "specversion": "1.0",
            "type": "ping",
            "source": "/test",
            "data": {
                "message": "hello"
            }
        }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_cloudevent_invalid_json() {
    let state = common::create_test_state().await;
    let server = common::create_test_server(state);

    let response = server
        .post("/")
        .content_type("application/json")
        .text("{ invalid json }")
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_cloudevent_with_custom_message() {
    let state = common::create_test_state().await;
    let server = common::create_test_server(state);

    let response = server
        .post("/")
        .content_type("application/json")
        .json(&json!({
            "specversion": "1.0",
            "id": "test-456",
            "type": "ping",
            "source": "/test",
            "data": {
                "message": "testing 123"
            }
        }))
        .await;

    response.assert_status_ok();
    response.assert_json(&json!({
        "reply": "pong: testing 123",
        "event_id": "test-456"
    }));
}
