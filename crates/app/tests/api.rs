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
async fn saved_report_json_survives_server_restart() {
    let store = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../helixscan-runs")
        .join(format!("saved-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&store).unwrap();
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let panel: helixscan_core::Panel =
        serde_json::from_reader(std::fs::File::open(workspace.join("data/panel.json")).unwrap())
            .unwrap();
    let result = helixscan_core::scan::ScanResult {
        calls: panel
            .variants
            .iter()
            .map(|variant| helixscan_core::genotype::call(variant, &[0; 4], "unclear"))
            .collect(),
        qc: helixscan_core::qc::Qc::default(),
        checks: helixscan_core::marker::SampleChecks::default(),
        elapsed_seconds: 0.01,
        engine: "CPU Scalar; 1 worker threads".into(),
    };
    let report = serde_json::json!({
        "report_id": "savedreport1",
        "report": helixscan_app::build_report(
            &panel,
            helixscan_gpu::Selection::cpu(result),
        ).unwrap(),
        "engine": {
            "description": "CPU Scalar; 1 worker threads", "fallback_reason": null,
            "gpu_device": null, "gpu_driver": null, "gpu_memory_mb": null,
            "workgroup": null, "memory_cap_mb": null
        },
        "file_size": 319,
        "filename": "fixture.fastq"
    });
    std::fs::write(
        store.join("savedreport1.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    // This router has no in-memory job matching the persisted report ID.
    let app = router(
        workspace.join("data/panel.json"),
        store.clone(),
        store.clone(),
        1,
    )
    .unwrap();
    let response = app
        .oneshot(
            Request::get("/api/scans/savedreport1/report.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved, report);
    std::fs::remove_dir_all(store).unwrap();
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
        "/print.css",
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
#[tokio::test]
async fn embedded_art_and_video_ranges_return_the_supplied_bytes() {
    let root = std::path::PathBuf::from(std::env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = router(
        root.join("data/panel.json"),
        root.join("testdata"),
        root.join("helixscan-runs/api-tests"),
        2,
    )
    .unwrap();
    for filename in [
        "00-helix.webp",
        "01-ancestry.webp",
        "02-body.webp",
        "03-heart.webp",
        "04-medicines.webp",
        "05-telomere.webp",
        "06-carrier.webp",
        "07-limits.webp",
        "08-cover.webp",
    ] {
        let expected = std::fs::read(root.join("web/art").join(filename)).unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/art/{filename}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{filename}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            expected.len().to_string()
        );
        let bytes = to_bytes(response.into_body(), 3_000_000).await.unwrap();
        assert_eq!(
            bytes.as_ref(),
            expected.as_slice(),
            "{filename} was changed"
        );
    }
    for unknown in [
        "00-helix.png",
        "08-cover.webp.exe",
        "../panel.json",
        "09-helix-loop.mp4x",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/art/{unknown}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{unknown}");
    }
    let video = std::fs::read(root.join("web/art/09-helix-loop.mp4")).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get("/art/09-helix-loop.mp4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(
        response.headers()[header::CONTENT_LENGTH],
        video.len().to_string()
    );
    assert_eq!(
        to_bytes(response.into_body(), 3_000_000).await.unwrap(),
        video
    );

    for (range, first, last) in [
        ("bytes=0-99", 0usize, 99usize),
        ("bytes=100-199", 100, 199),
        ("bytes=100-", 100, video.len() - 1),
        ("bytes=-37", video.len() - 37, video.len() - 1),
        (
            &format!("bytes=100-{}", video.len() + 100),
            100,
            video.len() - 1,
        ),
        (&format!("bytes=-{}", video.len() + 100), 0, video.len() - 1),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get("/art/09-helix-loop.mp4")
                    .header(header::RANGE, range)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT, "{range}");
        assert_eq!(
            response.headers()[header::CONTENT_RANGE],
            format!("bytes {first}-{last}/{}", video.len())
        );
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            (last - first + 1).to_string()
        );
        let body = to_bytes(response.into_body(), 3_000_000).await.unwrap();
        assert_eq!(body.as_ref(), &video[first..=last], "{range}");
    }
    for range in [
        "items=0-10",
        "bytes=invalid",
        "bytes=--",
        "bytes=-0",
        "bytes=10-9",
        "bytes=0-1,3-4",
        &format!("bytes={}-", video.len()),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get("/art/09-helix-loop.mp4")
                    .header(header::RANGE, range)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{range}"
        );
        assert_eq!(
            response.headers()[header::CONTENT_RANGE],
            format!("bytes */{}", video.len())
        );
        assert!(
            to_bytes(response.into_body(), 100)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
