use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use helixscan_app::api::router;
use tower::ServiceExt;

#[tokio::test]
async fn cpu_scan_streams_sse_and_persists_report() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = router(
        root.join("data/panel.json"),
        root.join("testdata"),
        root.join("helixscan-runs/api-tests"),
        4,
    )
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/scans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"input":"small.fastq","backend":"auto","threads":4}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10_000).await.unwrap()).unwrap();
    let id = created["id"].as_str().unwrap();
    let events = app
        .clone()
        .oneshot(
            Request::get(format!("/api/scans/{id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events.headers()[header::CONTENT_TYPE], "text/event-stream");
    let text = String::from_utf8(
        to_bytes(events.into_body(), 1_000_000)
            .await
            .unwrap()
            .into(),
    )
    .unwrap();
    assert!(text.contains("event: progress"));
    assert!(text.contains("\"state\":\"complete\""));
    let response = app
        .oneshot(
            Request::get(format!("/api/scans/{id}/report.json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let report: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 2_000_000).await.unwrap()).unwrap();
    assert_eq!(report["report"]["qc"]["reads"], 6500);
    assert_eq!(report["report"]["variants"].as_array().unwrap().len(), 77);
    assert_eq!(report["report"]["qc"]["panel_coverage"], 100.0);
}

#[tokio::test]
async fn assets_and_input_validation() {
    let root = std::path::PathBuf::from(std::env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = router(
        root.join("data/panel.json"),
        root.join("testdata"),
        root.join("helixscan-runs/api-tests"),
        2,
    )
    .unwrap();
    for path in [
        "/",
        "/app.js",
        "/helix.js",
        "/styles.css",
        "/responsive.css",
        "/visuals.css",
        "/vendor/three.module.js",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = to_bytes(response.into_body(), 2_000_000).await.unwrap();
        assert!(!body.is_empty());
    }
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/scans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"input":"../private.fastq","backend":"cpu"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app
        .oneshot(Request::get("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
