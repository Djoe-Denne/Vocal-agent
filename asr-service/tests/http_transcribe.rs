mod common;

use serde_json::json;

use common::setup_test_server;

#[tokio::test]
async fn transcribe_endpoint_returns_pipeline_response() -> Result<(), Box<dyn std::error::Error>> {
    let (_fixture, base_url, client) = setup_test_server().await?;

    let response = client
        .post(format!("{}/api/asr/transcribe", base_url))
        .json(&json!({
            "samples": [0.0, 0.1, 0.2, 0.3],
            "sample_rate_hz": 16000,
            "language_hint": "en",
            "session_id": "test-session"
        }))
        .send()
        .await?;

    assert!(response.status().is_success());
    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["session_id"], "test-session");
    assert!(body["transcript"].is_object());
    assert!(body["text"].as_str().is_some());

    Ok(())
}
