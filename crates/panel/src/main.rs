use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use helixscan_panel::{Ensembl, PanelBuilder};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "helixscan-panel", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long, default_value = "data/panel.seed.tsv")]
        seed: PathBuf,
        #[arg(long, default_value = "data/panel.json")]
        out: PathBuf,
        #[arg(long, default_value = "data/panel.drop.log")]
        drop_log: PathBuf,
        #[arg(long, default_value_t = 0)]
        limit: usize,
        #[arg(long, default_value_t = 150)]
        request_delay_ms: u64,
    },
}
fn main() -> Result<()> {
    let Args {
        command:
            Command::Build {
                seed,
                out,
                drop_log,
                limit,
                request_delay_ms,
            },
    } = Args::parse();
    let release = Ensembl::default()
        .get("info/data?")?
        .into_body()
        .read_json::<serde_json::Value>()?["releases"][0]
        .as_u64()
        .unwrap_or(0) as u32;
    let client = Ensembl {
        release,
        delay: Duration::from_millis(request_delay_ms.max(67)),
    };
    let mut builder = PanelBuilder::new(client);
    let mut panel = builder.build(&seed, limit)?;
    panel.pgx_rules = fs::read_to_string("data/pgx_rules.toml")?;
    fs::write(&out, serde_json::to_string_pretty(&panel)? + "\n")?;
    fs::write(&drop_log, builder.drops.join("\n") + "\n")?;
    println!(
        "kept {}, dropped {} (Ensembl release {})",
        panel.variants.len(),
        builder.drops.len(),
        release
    );
    if release == 0 {
        bail!("Ensembl did not report a release");
    }
    Ok(())
}
