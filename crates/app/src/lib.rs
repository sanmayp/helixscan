pub mod api;

use anyhow::Result;
use helixscan_core::{Panel, report::Report};
use std::{fs::File, path::Path};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum BackendArg {
    Auto,
    Cpu,
    Gpu,
}
impl From<BackendArg> for helixscan_gpu::Backend {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Auto => Self::Auto,
            BackendArg::Cpu => Self::Cpu,
            BackendArg::Gpu => Self::Gpu,
        }
    }
}

pub fn load_panel(path: &Path) -> Result<Panel> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

pub fn scan_web_cpu(
    input: &Path,
    panel: &Panel,
    threads: usize,
    context: &helixscan_core::scan::ScanContext,
    progress: &helixscan_core::scan::Progress,
) -> Result<helixscan_gpu::Selection> {
    let result = helixscan_core::scan::scan_path_with_context(
        input,
        panel,
        threads,
        Some(progress),
        helixscan_core::encode::runtime(),
        Some(context),
    )?;
    Ok(helixscan_gpu::Selection::cpu(result))
}

pub fn scan_command(
    input: &Path,
    panel: &Panel,
    backend: BackendArg,
    threads: usize,
    gpu_index: Option<usize>,
    gpu_mem_mb: Option<u64>,
) -> Result<helixscan_gpu::Selection> {
    let progress = helixscan_core::scan::Progress::new(panel.variants.len());
    helixscan_gpu::scan::scan_path(
        input,
        panel,
        threads,
        Some(&progress),
        backend.into(),
        helixscan_gpu::GpuConfig {
            gpu_index,
            gpu_mem_mb,
            ..Default::default()
        },
    )
}

pub fn build_report(panel: &Panel, selection: helixscan_gpu::Selection) -> Result<Report> {
    Ok(helixscan_core::report::report(
        panel,
        selection.result.calls,
        selection.result.qc,
        selection.result.engine,
        selection.result.elapsed_seconds,
        selection.result.checks,
    ))
}
