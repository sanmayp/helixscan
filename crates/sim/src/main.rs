use anyhow::Result;
use clap::Parser;
use helixscan_sim::{Config, Sex, parallel_fastq, truths};
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "helixscan-sim",
    version,
    about = "Reproducible synthetic FASTQ generator"
)]
struct Args {
    #[arg(long, default_value = "data/panel.json")]
    panel: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 100_000)]
    reads: u64,
    #[arg(long, alias = "length", default_value_t = 150)]
    len: usize,
    #[arg(long, default_value_t = 30)]
    depth: u32,
    #[arg(long, default_value_t = 7)]
    seed: u64,
    #[arg(long, default_value = "truth.json")]
    truth: PathBuf,
    #[arg(long)]
    population: Option<String>,
    #[arg(long, default_value = "auto")]
    sex: String,
    #[arg(long, default_value_t = 0.0)]
    telomere: f64,
}
#[derive(Serialize)]
struct TruthFile<'a> {
    reads: u64,
    seed: u64,
    read_len: usize,
    population: Option<String>,
    sex: String,
    variants: &'a [helixscan_sim::Truth],
    telomeric_fraction: f64,
}
fn main() -> Result<()> {
    let a = Args::parse();
    let mut reads = a.reads;
    if a.len < 31 {
        anyhow::bail!("--len must be at least 31");
    }
    let panel: helixscan_panel::Panel = serde_json::from_reader(File::open(&a.panel)?)?;
    let config = Config {
        seed: a.seed,
        depth: a.depth,
        read_len: a.len,
        population: a.population.clone(),
        sex: match a.sex.as_str() {
            "XX" | "xx" => Sex::Xx,
            "XY" | "xy" => Sex::Xy,
            _ => Sex::Auto,
        },
        telomeric_fraction: a.telomere,
    };
    let file = File::create(&a.out)?;
    let out = BufWriter::with_capacity(8 * 1024 * 1024, file);
    let mut out = out;
    let variants = truths(&panel, &config);
    reads = reads.max(variants.len() as u64 * config.depth as u64);
    parallel_fastq(&mut out, &panel, reads, &config, &variants)?;
    out.flush()?;
    let truth = TruthFile {
        reads,
        seed: a.seed,
        read_len: a.len,
        population: a.population,
        sex: a.sex,
        variants: &variants,
        telomeric_fraction: a.telomere,
    };
    serde_json::to_writer_pretty(File::create(&a.truth)?, &truth)?;
    println!(
        "simulated {} reads; truth for {} variants",
        reads,
        variants.len()
    );
    Ok(())
}
