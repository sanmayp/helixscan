// Run with: HELIXSCAN_CHROME=/path/to/chrome cargo test -p helixscan-app --features browser-test --test saved_report_browser
use anyhow::{Context, Result, bail};
use helixscan_app::api::router;
use std::{path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
    process::{Child, Command},
};

fn saved_fixture(store: &std::path::Path) -> (PathBuf, &'static str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let panel: helixscan_core::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let result = helixscan_core::scan::ScanResult {
        calls: panel
            .variants
            .iter()
            .map(|v| helixscan_core::genotype::call(v, &[0; 4], "unclear"))
            .collect(),
        qc: helixscan_core::qc::Qc {
            reads: 6500,
            bases: 975000,
            mean_len: 150.0,
            panel_coverage: 100.0,
            ..Default::default()
        },
        checks: helixscan_core::marker::SampleChecks::default(),
        elapsed_seconds: 0.123,
        engine: "CPU Scalar; 1 worker threads".into(),
    };
    let id = "browserreport1";
    let report = serde_json::json!({
        "report_id": id,
        "report": helixscan_app::build_report(&panel, helixscan_gpu::Selection::cpu(result)).unwrap(),
        "engine": { "description": null, "fallback_reason": null, "gpu_device": null,
            "gpu_driver": null, "gpu_memory_mb": null, "workgroup": null, "memory_cap_mb": null },
        "file_size": 319, "filename": "saved-browser.fastq"
    });
    std::fs::create_dir_all(store).unwrap();
    std::fs::write(
        store.join(format!("{id}.json")),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    (store.to_owned(), id)
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_report_html_displays_saved_values() -> Result<()> {
    let chrome = std::env::var_os("HELIXSCAN_CHROME")
        .expect("set HELIXSCAN_CHROME to a Chromium executable");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let store =
        std::env::temp_dir().join(format!("helixscan-browser-test-{}", uuid::Uuid::new_v4()));
    let profile = store.join("chrome-profile");
    let (store, id) = saved_fixture(&store);
    let app = router(
        root.join("data/panel.json"),
        store.clone(),
        store.clone(),
        1,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut browser = Command::new(chrome)
        .args([
            "--headless",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-background-networking",
            "--remote-debugging-port=0",
            &format!("--user-data-dir={}", profile.display()),
            "about:blank",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let result = browser_test(&mut browser, address, id).await;
    tokio::time::timeout(std::time::Duration::from_secs(10), browser.kill()).await??;
    server.abort();
    let _ = server.await;
    std::fs::remove_dir_all(store)?;
    result
}

async fn browser_test(browser: &mut Child, address: std::net::SocketAddr, id: &str) -> Result<()> {
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut lines = BufReader::new(browser.stderr.take().unwrap()).lines();
        loop {
            let line = lines
                .next_line()
                .await?
                .context("Chromium exited before reporting a DevTools address")?;
            if let Some(start) = line.find("DevTools listening on ") {
                let url = line[start + "DevTools listening on ".len()..].trim();
                return Ok::<_, anyhow::Error>(
                    url.split("/devtools/")
                        .next()
                        .context("DevTools URL")?
                        .replace("ws://", "http://"),
                );
            }
        }
    })
    .await
    .context("Chromium startup timed out")
    .and_then(|result| result)?;
    let script = format!(
        r#"
const assert = require('node:assert/strict');
const {{chromium}} = require('playwright');
(async () => {{
  const browser = await chromium.connectOverCDP({:?});
  const context = browser.contexts()[0];
  const page = context.pages()[0] || await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const responses = [];
  page.on('response', response => responses.push([response.status(), response.url()]));
  const navigation = await page.goto('http://{}/reports/{}/report');
  console.log('HTTP status:', navigation.status());
  let cards = [];
  for (let attempt = 0; attempt < 100; attempt++) {{
    cards = await page.locator('#qc-cards strong').allTextContents();
    if (cards.length === 6) break;
    await page.waitForTimeout(100);
  }}
  console.log('QC cards:', JSON.stringify(cards), 'errors:', JSON.stringify(errors));
  assert.equal(await page.locator('#report-file').textContent(), 'saved-browser.fastq · 319 B');
  assert.match(await page.locator('#report-engine').textContent(), /CPU Scalar; 1 worker threads · 0\.123 s/);
  assert.deepEqual(cards.slice(0, 3), ['6,500', '975,000', '150.0']);
  assert.equal(cards[5], '100.0%');
  assert.equal(await page.locator('#variant-rows tr').count(), 77);
  assert.ok((await page.locator('#depth-chart').textContent()).includes('No variants met the minimum coverage.'));
  assert.ok((await page.locator('#traits').textContent()).includes('Pharmacogenetic interpretations'));
  assert.ok((await page.locator('#limitations').textContent()).includes('CYP2D6'));
  assert.deepEqual(errors, []);
  assert.doesNotMatch(await page.locator('#report-page').textContent(), /undefined|NaN/);
  console.log('Saved report browser check passed: real filename, size, engine, QC, 77 rows, and complete sections.');
  await context.close();
  await browser.close();
}})().catch(error => {{ console.error(error); process.exit(1); }});
"#,
        output, address, id
    );
    let mut node = Command::new("node")
        .current_dir(
            std::env::var("PLAYWRIGHT_DIR")
                .expect("set PLAYWRIGHT_DIR to a directory containing Playwright"),
        )
        .args(["--input-type=commonjs", "-e", &script])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(60), node.wait())
        .await
        .context("Playwright timed out")??;
    if !status.success() {
        bail!("browser check exited with {status}");
    }
    Ok(())
}
