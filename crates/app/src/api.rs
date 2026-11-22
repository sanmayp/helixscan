use crate::{BackendArg, load_panel, scan_command};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::sse::Event,
    response::{Html, IntoResponse, Response, sse::Sse},
    routing::{get, post},
};
use helixscan_core::{Panel, report::Report, scan::Progress};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
const FINAL_EVENT: &str = "event: progress\n";
fn completed_snapshot(report: &JobReport) -> serde_json::Value {
    serde_json::json!({
        "state": "complete", "bytes": report.file_size,
        "reads": report.report.qc.reads, "bases": report.report.qc.bases,
        "total_bytes": report.file_size,
        "elapsed_seconds": report.report.elapsed_seconds, "hits": completed_hits(report),
        "result": report,
    })
}
fn completed_hits(report: &JobReport) -> Vec<Hit> {
    report
        .report
        .variants
        .iter()
        .filter(|call| call.depth > 0)
        .map(|call| Hit {
            gene: call.gene.clone(),
            rsid: call.rsid.clone(),
            ref_votes: call.ref_votes,
            alt_votes: call.alt_votes.iter().sum(),
        })
        .collect()
}
use tokio::sync::watch;
use uuid::Uuid;

const UPLOAD_WRITE_SIZE: usize = 1024 * 1024;
const DEFAULT_UPLOAD_LIMIT: u64 = 64 * 1024 * 1024 * 1024;

const INDEX: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const HELIX_JS: &str = include_str!("../../../web/helix.js");
const STYLES: &str = include_str!("../../../web/styles.css");
const VISUALS: &str = include_str!("../../../web/visuals.css");
const RESPONSIVE: &str = include_str!("../../../web/responsive.css");
const PRINT: &str = include_str!("../../../web/print.css");
const THREE: &str = include_str!("../../../web/vendor/three.module.js");
const HELIX_POSTER: &[u8] = include_bytes!("../../../web/art/00-helix.webp");
const HELIX_VIDEO: &[u8] = include_bytes!("../../../web/art/09-helix-loop.mp4");

#[derive(Clone)]
pub struct AppState {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
    panel: Arc<Panel>,
    context: Arc<helixscan_core::scan::ScanContext>,
    input_dir: Arc<PathBuf>,
    store_dir: Arc<PathBuf>,
    threads: usize,
    upload_limit: u64,
}
#[derive(Clone)]
struct Job {
    progress: Progress,
    threads: usize,
    file_size: u64,
    started: Arc<Mutex<Option<Instant>>>,
    outcome: Arc<Mutex<Option<Result<JobReport, String>>>>,
    cancel: watch::Sender<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobReport {
    pub report_id: String,
    pub report: Report,
    pub engine: Engine,
    pub file_size: u64,
    pub filename: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Engine {
    pub description: Option<String>,
    pub fallback_reason: Option<String>,
    pub gpu_device: Option<String>,
    pub gpu_driver: Option<String>,
    pub gpu_memory_mb: Option<u64>,
    pub workgroup: Option<u32>,
    pub memory_cap_mb: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    state: &'static str,
    threads: usize,
    bytes: u64,
    reads: u64,
    bases: u64,
    total_bytes: u64,
    elapsed_seconds: f64,
    hits: Vec<Hit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<JobReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Hit {
    gene: String,
    rsid: String,
    ref_votes: u32,
    alt_votes: u32,
}
#[derive(Deserialize)]
struct ScanRequest {
    input: String,
    #[serde(default)]
    backend: BackendRequest,
    threads: Option<usize>,
    gpu_mem_mb: Option<u64>,
}
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
enum BackendRequest {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

pub fn router(
    panel_path: PathBuf,
    input_dir: PathBuf,
    store_dir: PathBuf,
    threads: usize,
) -> anyhow::Result<Router> {
    router_with_upload_limit(
        panel_path,
        input_dir,
        store_dir,
        threads,
        DEFAULT_UPLOAD_LIMIT,
    )
}

pub fn router_with_upload_limit(
    panel_path: PathBuf,
    input_dir: PathBuf,
    store_dir: PathBuf,
    threads: usize,
    upload_limit: u64,
) -> anyhow::Result<Router> {
    anyhow::ensure!(upload_limit > 0, "upload limit must be positive");
    fs::create_dir_all(&store_dir)?;
    let panel = load_panel(&panel_path)?;
    let state = AppState {
        jobs: Arc::new(Mutex::new(HashMap::new())),
        panel: Arc::new(panel.clone()),
        context: Arc::new(helixscan_core::scan::ScanContext::new(
            &panel,
            helixscan_core::encode::runtime(),
        )),
        input_dir: Arc::new(input_dir),
        store_dir: Arc::new(store_dir),
        threads,
        upload_limit,
    };
    Ok(Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/helix.js", get(helix_js))
        .route("/styles.css", get(styles_css))
        .route("/responsive.css", get(responsive_css))
        .route("/visuals.css", get(visuals_css))
        .route("/print.css", get(print_css))
        .route("/vendor/three.module.js", get(three_js))
        .route("/art/{filename}", get(art))
        .route("/api/panel", get(panel_summary))
        .route("/api/telemetry", get(telemetry))
        .route("/api/files", get(files))
        .route(
            "/api/uploads",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/scans", post(create_scan))
        .route("/api/scans/{id}", get(scan_snapshot))
        .route("/api/scans/{id}/events", get(events))
        .route("/reports/{id}", get(stored_snapshot))
        .route("/reports/{id}/report", get(report_html))
        .route("/reports/{id}/events", get(stored_events))
        .route("/api/scans/{id}/report.json", get(report_json))
        .route("/api/scans/{id}/report.html", get(report_html))
        .with_state(state)
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024 * 1024)))
}

struct TemporaryUpload {
    path: PathBuf,
    persisted: bool,
}
impl TemporaryUpload {
    fn persist(mut self, destination: &std::path::Path) -> io::Result<()> {
        fs::rename(&self.path, destination)?;
        self.persisted = true;
        Ok(())
    }
}
impl Drop for TemporaryUpload {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

async fn app_js() -> impl axum::response::IntoResponse {
    javascript(APP_JS)
}
async fn three_js() -> impl axum::response::IntoResponse {
    javascript(THREE)
}
async fn helix_js() -> impl axum::response::IntoResponse {
    javascript(HELIX_JS)
}
fn javascript(source: &'static str) -> ([(axum::http::HeaderName, &'static str); 2], &'static str) {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        source,
    )
}
async fn styles_css() -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES)
}
async fn responsive_css() -> impl axum::response::IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        RESPONSIVE,
    )
}
async fn visuals_css() -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], VISUALS)
}
async fn print_css() -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], PRINT)
}
async fn index() -> Html<&'static str> {
    Html(INDEX)
}

async fn art(Path(filename): Path<String>, request_headers: HeaderMap) -> Response {
    // Embed an exact set of named assets; a URL never becomes a filesystem path.
    let body: &[u8] = match filename.as_str() {
        "00-helix.webp" => HELIX_POSTER,
        "01-ancestry.webp" => include_bytes!("../../../web/art/01-ancestry.webp"),
        "02-body.webp" => include_bytes!("../../../web/art/02-body.webp"),
        "03-heart.webp" => include_bytes!("../../../web/art/03-heart.webp"),
        "04-medicines.webp" => include_bytes!("../../../web/art/04-medicines.webp"),
        "05-telomere.webp" => include_bytes!("../../../web/art/05-telomere.webp"),
        "06-carrier.webp" => include_bytes!("../../../web/art/06-carrier.webp"),
        "07-limits.webp" => include_bytes!("../../../web/art/07-limits.webp"),
        "08-cover.webp" => include_bytes!("../../../web/art/08-cover.webp"),
        "09-helix-loop.mp4" => HELIX_VIDEO,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let range_supported = filename == "09-helix-loop.mp4";
    let content_type = if range_supported {
        "video/mp4"
    } else {
        "image/webp"
    };
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    if range_supported {
        response = response.header(header::ACCEPT_RANGES, "bytes");
    }
    let length = body.len();
    let Some(range) = request_headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
    else {
        return response
            .header(header::CONTENT_LENGTH, length.to_string())
            .body(Body::from(body))
            .unwrap();
    };
    if !range_supported {
        return response
            .header(header::CONTENT_LENGTH, length.to_string())
            .body(Body::from(body))
            .unwrap();
    }
    let Some(spec) = range.strip_prefix("bytes=") else {
        return range_not_satisfiable(length);
    };
    // A multi-range response would require multipart/byteranges; reject it instead
    // of accidentally coalescing two ranges into unrelated bytes.
    if spec.contains(',') {
        return range_not_satisfiable(length);
    }
    let Some((start, end)) = spec.split_once('-') else {
        return range_not_satisfiable(length);
    };
    if start.is_empty() {
        let Ok(suffix) = end.parse::<usize>() else {
            return range_not_satisfiable(length);
        };
        if suffix == 0 {
            return range_not_satisfiable(length);
        }
        let first = length.saturating_sub(suffix);
        return ranged_body(response, body, first, length - 1, length);
    }
    let Ok(first) = start.parse::<usize>() else {
        return range_not_satisfiable(length);
    };
    if first >= length {
        return range_not_satisfiable(length);
    }
    let last = if end.is_empty() {
        length - 1
    } else {
        match end.parse::<usize>() {
            Ok(value) => value.min(length - 1),
            Err(_) => return range_not_satisfiable(length),
        }
    };
    if first > last {
        return range_not_satisfiable(length);
    }
    ranged_body(response, body, first, last, length)
}

fn ranged_body(
    mut response: axum::http::response::Builder,
    body: &'static [u8],
    first: usize,
    last: usize,
    length: usize,
) -> Response {
    response = response
        .status(StatusCode::PARTIAL_CONTENT)
        .header(
            header::CONTENT_RANGE,
            format!("bytes {first}-{last}/{length}"),
        )
        .header(header::CONTENT_LENGTH, (last - first + 1).to_string());
    response.body(Body::from(&body[first..=last])).unwrap()
}

fn range_not_satisfiable(length: usize) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{length}"))
        .body(Body::empty())
        .unwrap()
}

#[derive(Serialize)]
struct PanelSummary {
    variants: usize,
    genes: Vec<String>,
    searched_traits: usize,
}
async fn panel_summary(State(state): State<AppState>) -> Json<PanelSummary> {
    let mut genes = state
        .panel
        .variants
        .iter()
        .map(|v| v.gene.clone())
        .collect::<Vec<_>>();
    genes.sort();
    genes.dedup();
    Json(PanelSummary {
        variants: state.panel.variants.len(),
        genes,
        searched_traits: state.panel.searched_traits.len(),
    })
}

#[derive(Serialize, Default)]
pub struct Telemetry {
    pub cpus: Vec<f64>,
    pub gpu_busy_percent: Option<f64>,
    pub simd: String,
    pub workers: usize,
    pub reader_threads: usize,
}
pub fn read_cpu_utilization() -> Vec<f64> {
    let Ok(sample) = fs::read_to_string("/proc/stat") else {
        return vec![];
    };
    let fields = |line: &str| -> Option<(String, Vec<u64>)> {
        let mut split = line.split_whitespace();
        let name = split.next()?.to_owned();
        let values = split.filter_map(|x| x.parse().ok()).collect::<Vec<u64>>();
        Some((name, values))
    };
    type CpuSample = (String, Vec<u64>);
    static PREVIOUS: Mutex<Option<Vec<CpuSample>>> = Mutex::new(None);
    let current = sample
        .lines()
        .filter(|line| line.starts_with("cpu"))
        .filter_map(fields)
        .filter(|(name, _)| name.len() > 3 && name[3..].bytes().all(|b| b.is_ascii_digit()))
        .collect::<Vec<_>>();
    let mut previous = PREVIOUS.lock().unwrap_or_else(|e| e.into_inner());
    let last = previous.take();
    *previous = Some(current.clone());
    last.zip(Some(&current))
        .map(|(last, current)| {
            last.into_iter()
                .zip(current)
                .map(|((_name, before), (_, after))| {
                    let deltas = after
                        .iter()
                        .zip(before)
                        .map(|(a, b)| a.saturating_sub(b))
                        .collect::<Vec<_>>();
                    let total = deltas.iter().sum::<u64>();
                    let idle =
                        deltas.get(3).copied().unwrap_or(0) + deltas.get(4).copied().unwrap_or(0);
                    (1.0 - idle as f64 / total.max(1) as f64) * 100.0
                })
                .collect()
        })
        .unwrap_or_default()
}
async fn telemetry(State(state): State<AppState>) -> Json<Telemetry> {
    Json(Telemetry {
        cpus: read_cpu_utilization(),
        gpu_busy_percent: gpu_busy(),
        simd: format!("{:?}", helixscan_core::encode::runtime()),
        workers: state.threads.max(1),
        reader_threads: 1,
    })
}
fn gpu_busy() -> Option<f64> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .and_then(|s| s.trim().parse().ok())
    })?
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    size: u64,
}
fn input_filename(name: &str) -> Option<&str> {
    let supported = name.ends_with(".fastq")
        || name.ends_with(".fq")
        || name.ends_with(".fastq.gz")
        || name.ends_with(".fq.gz");
    (supported
        && !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
    .then_some(name)
}
async fn files(State(state): State<AppState>) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let mut entries = fs::read_dir(state.input_dir.as_path())
        .map_err(server_error)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            input_filename(&name)?;
            Some(FileEntry {
                name,
                size: entry.metadata().ok()?.len(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(entries))
}
fn server_error(error: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
struct ApiError(StatusCode, String);
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(serde_json::json!({"error": self.1}))).into_response()
    }
}

async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<FileEntry>, ApiError> {
    use tokio::io::AsyncWriteExt;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid upload: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let original = field
            .file_name()
            .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "missing filename".into()))?;
        if input_filename(original).is_none() {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "filename must use letters, numbers, '.', '_' or '-', and end in FASTQ/fq (optionally .gz)".into(),
            ));
        }
        let stored = format!("upload-{}{}", Uuid::new_v4().simple(), original);
        let path = state.input_dir.join(&stored);
        let temporary_path = state
            .input_dir
            .join(format!(".upload-{}.tmp", Uuid::new_v4().simple()));
        // Create synchronously so cancellation cannot leave a racing async open behind.
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(server_error)?;
        let temporary = TemporaryUpload {
            path: temporary_path,
            persisted: false,
        };
        let mut output = tokio::fs::File::from_std(file);
        let mut field = field;
        let mut buffer = Vec::with_capacity(UPLOAD_WRITE_SIZE);
        let mut size = 0_u64;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("upload read failed: {e}")))?
        {
            let chunk_size = chunk.len() as u64;
            if chunk_size > state.upload_limit - size {
                return Err(ApiError(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    format!("upload exceeds the {} byte limit", state.upload_limit),
                ));
            }
            size += chunk_size;
            let mut remaining = chunk.as_ref();
            while !remaining.is_empty() {
                let available = UPLOAD_WRITE_SIZE - buffer.len();
                let count = available.min(remaining.len());
                buffer.extend_from_slice(&remaining[..count]);
                remaining = &remaining[count..];
                if buffer.len() == UPLOAD_WRITE_SIZE {
                    output.write_all(&buffer).await.map_err(server_error)?;
                    buffer.clear();
                }
            }
        }
        output.write_all(&buffer).await.map_err(server_error)?;
        output.flush().await.map_err(server_error)?;
        drop(output);
        temporary.persist(&path).map_err(server_error)?;
        return Ok(Json(FileEntry { name: stored, size }));
    }
    Err(ApiError(StatusCode::BAD_REQUEST, "no file field".into()))
}

fn insert_job(state: &AppState, id: String, job: Job) {
    state
        .jobs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, job);
}
async fn create_scan(
    State(state): State<AppState>,
    Json(request): Json<ScanRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.threads == Some(0) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "threads must be positive".into(),
        ));
    }
    let filename = input_filename(&request.input)
        .ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "input is not an allowed FASTQ filename in the configured folder".into(),
            )
        })?
        .to_owned();
    let input = state.input_dir.join(&filename);
    let metadata = fs::metadata(&input)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "input file not found".into()))?;
    let threads = request.threads.unwrap_or(state.threads).max(1);
    // The web default is CPU with runtime SIMD; GPU is an explicit opt-in.
    // The CLI `bench` command remains the measured auto-backend comparison.
    let backend = match request.backend {
        BackendRequest::Auto => BackendArg::Cpu,
        BackendRequest::Cpu => BackendArg::Cpu,
        BackendRequest::Gpu => BackendArg::Gpu,
    };
    let path = state
        .store_dir
        .join(format!("{}.json", Uuid::new_v4().simple()));
    let output_stem = path.file_stem().unwrap().to_string_lossy().into_owned();
    let progress = Progress::new(state.panel.variants.len());
    let (cancel, _receiver) = watch::channel(false);
    let job = Job {
        progress,
        threads,
        file_size: metadata.len(),
        started: Arc::new(Mutex::new(Some(Instant::now()))),
        outcome: Arc::new(Mutex::new(None)),
        cancel,
    };
    insert_job(&state, output_stem.clone(), job.clone());
    let gpu_mem_mb = request.gpu_mem_mb;
    let id = output_stem.clone();
    tokio::task::spawn_blocking(move || {
        let run = JobRun {
            input,
            filename,
            output: path,
            backend,
            threads,
            gpu_mem_mb,
        };
        run_job(&state, run, job, id);
    });
    Ok(Json(serde_json::json!({
        "id": output_stem,
        "report_url": format!("/reports/{output_stem}/report")
    })))
}
struct JobRun {
    input: PathBuf,
    filename: String,
    output: PathBuf,
    backend: BackendArg,
    threads: usize,
    gpu_mem_mb: Option<u64>,
}
fn run_job(state: &AppState, run: JobRun, job: Job, id: String) {
    let result = match run.backend {
        BackendArg::Gpu => scan_command(
            &run.input,
            &state.panel,
            run.backend,
            run.threads,
            None,
            run.gpu_mem_mb,
        ),
        BackendArg::Auto | BackendArg::Cpu => crate::scan_web_cpu(
            &run.input,
            &state.panel,
            run.threads,
            &state.context,
            &job.progress,
        ),
    };
    let outcome = result.and_then(|selection| {
        let report = crate::build_report(&state.panel, selection.clone())?;
        let stats = selection.gpu_stats;
        let engine = Engine {
            description: Some(report.engine.clone()),
            fallback_reason: selection.fallback_reason,
            gpu_device: stats.as_ref().map(|s| s.device.clone()),
            gpu_driver: stats.as_ref().map(|s| s.driver.clone()),
            gpu_memory_mb: stats.as_ref().and_then(|s| s.memory_mb),
            workgroup: stats.as_ref().map(|s| s.workgroup),
            memory_cap_mb: stats.as_ref().map(|s| s.memory_cap_mb),
        };
        let job_report = JobReport {
            report_id: id,
            report,
            engine,
            file_size: job.file_size,
            filename: run.filename,
        };
        fs::write(&run.output, serde_json::to_vec_pretty(&job_report)?)?;
        Ok(job_report)
    });
    *job.outcome.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(outcome.map_err(|e| e.to_string()));
    let _ = job.cancel.send(true);
}
fn lookup(state: &AppState, id: &str) -> Result<Job, ApiError> {
    state
        .jobs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
        .ok_or(ApiError(StatusCode::NOT_FOUND, "scan not found".into()))
}
async fn scan_snapshot(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Snapshot>, ApiError> {
    let job = lookup(&state, &id)?;
    Ok(Json(snapshot(&state, &job)))
}
fn snapshot(state: &AppState, job: &Job) -> Snapshot {
    let outcome = job
        .outcome
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(Ok(report)) = &outcome {
        return Snapshot {
            state: "complete",
            threads: job.threads,
            bytes: report.file_size,
            reads: report.report.qc.reads,
            bases: report.report.qc.bases,
            total_bytes: report.file_size,
            elapsed_seconds: report.report.elapsed_seconds,
            hits: completed_hits(report),
            result: Some(report.clone()),
            error: None,
        };
    }
    let state_label = match &outcome {
        Some(Ok(_)) => "complete",
        Some(Err(_)) => "failed",
        None => "running",
    };
    Snapshot {
        state: state_label,
        threads: job.threads,
        bytes: job
            .progress
            .bytes
            .load(std::sync::atomic::Ordering::Relaxed),
        reads: job
            .progress
            .reads
            .load(std::sync::atomic::Ordering::Relaxed),
        bases: job
            .progress
            .bases
            .load(std::sync::atomic::Ordering::Relaxed),
        total_bytes: job.file_size,
        elapsed_seconds: job
            .started
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map_or(0.0, |start| start.elapsed().as_secs_f64()),
        hits: state
            .panel
            .variants
            .iter()
            .zip(job.progress.votes.iter())
            .filter(|(_, counts)| {
                counts
                    .iter()
                    .any(|count| count.load(std::sync::atomic::Ordering::Relaxed) > 0)
            })
            .map(|(v, counts)| Hit {
                gene: v.gene.clone(),
                rsid: v.rsid.clone(),
                ref_votes: counts[0].load(std::sync::atomic::Ordering::Relaxed),
                alt_votes: counts[1..]
                    .iter()
                    .map(|count| count.load(std::sync::atomic::Ordering::Relaxed))
                    .sum(),
            })
            .collect(),
        error: outcome.as_ref().and_then(|x| x.as_ref().err().cloned()),
        result: outcome.and_then(Result::ok),
    }
}
async fn events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let job = lookup(&state, &id)?;
    let mut receiver = job.cancel.subscribe();
    let stream = async_stream::stream! {
        loop {
            let current = snapshot(&state, &job);
            let finished = current.state != "running";
            let data = serde_json::to_string(&current).unwrap_or_else(|_| "{}".into());
            yield Ok::<Event, Infallible>(Event::default().event("progress").data(data));
            if finished { break; }
            tokio::select! {
                result = receiver.changed() => {
                    if result.is_err() { break; }
                    receiver.borrow_and_update();
                },
                () = tokio::time::sleep(std::time::Duration::from_millis(200)) => {},
            }
        }
    };
    Ok(Sse::new(stream))
}
async fn report_json(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JobReport>, ApiError> {
    if let Some(report) = saved_report(&state, &id) {
        let report = report.map_err(|error| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("saved report is invalid: {error}"),
            )
        })?;
        return Ok(Json(report));
    }
    let job = lookup(&state, &id)?;
    let outcome = job
        .outcome
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    match outcome {
        Some(Ok(report)) => Ok(Json(report)),
        Some(Err(error)) => Err(ApiError(StatusCode::CONFLICT, error)),
        None => Err(ApiError(StatusCode::CONFLICT, "report is not ready".into())),
    }
}
async fn report_html(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, ApiError> {
    let report = match saved_report(&state, &id) {
        Some(Ok(report)) => report,
        Some(Err(error)) => return Err(ApiError(StatusCode::INTERNAL_SERVER_ERROR, error)),
        None => {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "running scans have no standalone report yet".into(),
            ));
        }
    };
    let script = format!(
        "<script>loadReport({});</script>",
        serde_json::to_string(&report.report_id).unwrap()
    );
    let body = INDEX.replace("</body>", &format!("{script}</body>"));
    Ok(Html(body))
}

fn saved_report(state: &AppState, id: &str) -> Option<Result<JobReport, String>> {
    if !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let content = fs::read_to_string(state.store_dir.join(format!("{id}.json"))).ok()?;
    Some(serde_json::from_str(&content).map_err(|error| error.to_string()))
}

async fn stored_snapshot(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let snapshot = match saved_report(&state, &id) {
        Some(Ok(report)) => completed_snapshot(&report),
        Some(Err(error)) => serde_json::json!({"state":"failed","error":error}),
        None => {
            let job = lookup(&state, &id)?;
            serde_json::to_value(snapshot(&state, &job)).map_err(server_error)?
        }
    };
    Ok(Json(snapshot))
}

async fn stored_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    if let Some(Ok(report)) = saved_report(&state, &id) {
        let event = format!(
            "{FINAL_EVENT}data: {}\n\n",
            serde_json::to_string(&completed_snapshot(&report)).unwrap()
        );
        return Ok(([("content-type", "text/event-stream")], event));
    }
    Err(ApiError(
        StatusCode::NOT_FOUND,
        "saved scan is not complete".into(),
    ))
}
