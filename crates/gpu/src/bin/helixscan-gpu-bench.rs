use anyhow::Result;
use clap::Parser;
use helixscan_gpu::Backend;
use std::{fs::File, path::PathBuf, time::Instant};

/// Measure complete scanning paths, including GPU readback and CPU voting.
#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value = "data/panel.json")]
    panel: PathBuf,
    #[arg(long, default_value_t = 11)]
    threads: usize,
    #[arg(long)]
    gpu_mem_mb: Option<u64>,
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    tune: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let panel: helixscan_core::Panel = serde_json::from_reader(File::open(&args.panel)?)?;
    let simd = helixscan_core::encode::runtime();
    let bytes = std::fs::metadata(&args.input)?.len();
    println!("backend\tbytes\tseconds\tMiB/s\treads\tdevice\tworkgroup\treason");
    let start = Instant::now();
    let cpu =
        helixscan_core::scan::scan_path_with_simd(&args.input, &panel, args.threads, None, simd)?;
    println!(
        "CPU\t{bytes}\t{:.6}\t{:.3}\t{}\t{simd:?}\t-\t-",
        start.elapsed().as_secs_f64(),
        bytes as f64 / cpu.elapsed_seconds / (1024.0 * 1024.0),
        cpu.qc.reads,
    );
    for backend in [Backend::Gpu, Backend::Auto] {
        let start = Instant::now();
        let selection = helixscan_gpu::scan::scan_path(
            &args.input,
            &panel,
            args.threads,
            None,
            backend,
            helixscan_gpu::GpuConfig {
                gpu_mem_mb: args.gpu_mem_mb,
                tune: args.tune,
                ..Default::default()
            },
        )?;
        if selection.result.calls != cpu.calls {
            anyhow::bail!("GPU benchmark disagrees with the CPU calls");
        }
        let stats = selection.gpu_stats;
        println!(
            "{backend:?}\t{bytes}\t{:.6}\t{:.3}\t{}\t{}\t{}\t{}",
            start.elapsed().as_secs_f64(),
            bytes as f64 / selection.result.elapsed_seconds / (1024.0 * 1024.0),
            selection.result.qc.reads,
            selection.gpu_device.as_deref().unwrap_or("unavailable"),
            stats
                .as_ref()
                .map_or("-".into(), |s| s.workgroup.to_string()),
            selection.fallback_reason.as_deref().unwrap_or("kept GPU")
        );
    }
    Ok(())
}
