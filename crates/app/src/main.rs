use anyhow::Result;
use clap::{Parser, Subcommand};
use helixscan_app::BackendArg;
use std::{fs::File, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "helixscan",
    about = "Research variant scanning and an offline web interface"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Scan {
        input: PathBuf,
        #[arg(long, default_value = "data/panel.json")]
        panel: PathBuf,
        #[arg(long, default_value = "report.json")]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = BackendArg::Auto)]
        backend: BackendArg,
        #[arg(long, default_value_t = 11)]
        threads: usize,
        #[arg(long)]
        gpu_index: Option<usize>,
        #[arg(long)]
        gpu_mem_mb: Option<u64>,
    },
    Serve {
        #[arg(long, default_value_t = 8780)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        #[arg(long, default_value = "data/panel.json")]
        panel: PathBuf,
        #[arg(long, default_value = "testdata")]
        input_dir: PathBuf,
        #[arg(long, default_value = "helixscan-runs")]
        store_dir: PathBuf,
        #[arg(long, default_value_t = 11)]
        threads: usize,
        #[arg(long, default_value_t = 64)]
        max_upload_gib: u64,
    },
    Bench {
        input: PathBuf,
        #[arg(long, default_value = "data/panel.json")]
        panel: PathBuf,
        #[arg(long, default_value_t = 11)]
        threads: usize,
        #[arg(long, default_value_t = 1)]
        repeats: usize,
    },
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Scan {
            input,
            panel,
            out,
            backend,
            threads,
            gpu_index,
            gpu_mem_mb,
        } => {
            if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let panel = helixscan_app::load_panel(&panel)?;
            let selection = helixscan_app::scan_command(
                &input, &panel, backend, threads, gpu_index, gpu_mem_mb,
            )?;
            if let Some(reason) = &selection.fallback_reason {
                eprintln!("{reason}");
            }
            let report = helixscan_app::build_report(&panel, selection)?;
            serde_json::to_writer_pretty(File::create(&out)?, &report)?;
            println!(
                "{}: {} reads; {} called; panel coverage {:.1}%",
                out.display(),
                report.qc.reads,
                report.variants.iter().filter(|c| c.called).count(),
                report.qc.panel_coverage
            );
        }
        Command::Serve {
            port,
            bind,
            panel,
            input_dir,
            store_dir,
            threads,
            max_upload_gib,
        } => {
            if bind != "127.0.0.1" && bind != "0.0.0.0" {
                anyhow::bail!("--bind must be 127.0.0.1 or 0.0.0.0");
            }
            let upload_limit = max_upload_gib
                .checked_mul(1024 * 1024 * 1024)
                .filter(|limit| *limit > 0)
                .ok_or_else(|| {
                    anyhow::anyhow!("--max-upload-gib must be between 1 and 16777216")
                })?;
            let router = helixscan_app::api::router_with_upload_limit(
                panel,
                input_dir,
                store_dir,
                threads,
                upload_limit,
            )?;
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(async move {
                    let listener = tokio::net::TcpListener::bind((bind.as_str(), port)).await?;
                    println!("HelixScan research UI: http://{}:{}", bind, port);
                    axum::serve(listener, router).await?;
                    anyhow::Ok(())
                })?;
        }
        Command::Bench {
            input,
            panel,
            threads,
            repeats,
        } => {
            let panel = helixscan_app::load_panel(&panel)?;
            let bytes = std::fs::metadata(&input)?.len();
            let levels = [
                helixscan_core::encode::SimdLevel::Scalar,
                helixscan_core::encode::SimdLevel::Avx2,
                helixscan_core::encode::SimdLevel::Avx512,
            ];
            println!("level\tseconds\tMiB/s\treads\tcalls");
            for level in levels.into_iter().filter(|level| level.available()) {
                for _ in 0..repeats.max(1) {
                    let result = helixscan_core::scan::scan_path_with_simd(
                        &input, &panel, threads, None, level,
                    )?;
                    println!(
                        "{level:?}\t{:.3}\t{:.3}\t{}\t{}",
                        result.elapsed_seconds,
                        bytes as f64 / result.elapsed_seconds / (1024.0 * 1024.0),
                        result.qc.reads,
                        result.calls.iter().filter(|c| c.called).count()
                    );
                }
            }
            for backend in [BackendArg::Gpu, BackendArg::Auto] {
                let selection =
                    helixscan_app::scan_command(&input, &panel, backend, threads, None, None)?;
                println!(
                    "{backend:?}\t{:.3}\t{:.3}\t{}\t{}",
                    selection.result.elapsed_seconds,
                    bytes as f64 / selection.result.elapsed_seconds / (1024.0 * 1024.0),
                    selection.result.qc.reads,
                    selection.result.calls.iter().filter(|c| c.called).count()
                );
            }
        }
    }
    Ok(())
}
