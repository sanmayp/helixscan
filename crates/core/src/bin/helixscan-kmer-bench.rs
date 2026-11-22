use anyhow::Result;
use clap::{Parser, ValueEnum};
use helixscan_core::{
    Panel,
    encode::{KmerEncoder, SimdLevel, encode_into},
    scan::scan_path_with_simd,
};
use std::{fs::File, path::PathBuf, time::Instant};
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Level {
    Scalar,
    Avx2,
    Avx512,
}
impl From<Level> for SimdLevel {
    fn from(value: Level) -> Self {
        match value {
            Level::Scalar => Self::Scalar,
            Level::Avx2 => Self::Avx2,
            Level::Avx512 => Self::Avx512,
        }
    }
}
#[derive(Parser)]
#[command(
    name = "helixscan-kmer-bench",
    about = "Measure real scalar/AVX2/AVX-512 encoding and scan paths"
)]
struct Args {
    #[arg(long, default_value = "data/panel.json")]
    panel: PathBuf,
    #[arg(long, default_value = "testdata/medium.fastq")]
    input: PathBuf,
    #[arg(long, default_value_t = 11)]
    threads: usize,
    #[arg(long)]
    encoding_mb: Option<usize>,
    #[arg(long, default_value_t = 1)]
    repeats: usize,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let panel: Panel = serde_json::from_reader(File::open(&args.panel)?)?;
    if !args.input.exists() {
        anyhow::bail!("benchmark fixture does not exist: {}", args.input.display());
    }
    let mb = args.encoding_mb.unwrap_or(256);
    let bytes = mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| anyhow::anyhow!("encoding benchmark size overflow"))?;
    let sequence: Vec<u8> = (0..bytes).map(|i| b"ACGTN"[(i * 17 + i / 7) % 5]).collect();
    println!("input\tlevel\tbytes\tseconds\tMiB/s\treads\tagreement\tengine");
    let mut baseline: Option<(String, String)> = None;
    for level in [Level::Scalar, Level::Avx2, Level::Avx512] {
        let simd: SimdLevel = level.into();
        if !simd.available() {
            println!("encoding\t{level:?}\t0\t0\t0\t0\tunavailable\tCPU");
            continue;
        }
        let mut encoded = Vec::new();
        let mut keys = Vec::new();
        let start = Instant::now();
        encode_into(&sequence, &mut encoded, simd).map_err(anyhow::Error::msg)?;
        let mut key_encoder = KmerEncoder::new(simd);
        key_encoder.kmer_keys_into(&sequence, &mut keys);
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "encoding\t{level:?}\t{bytes}\t{elapsed:.6}\t{:.3}\t0\tkeys={}\tCPU",
            bytes as f64 / elapsed / (1024.0 * 1024.0),
            keys.len()
        );
        for _ in 0..args.repeats.max(1) {
            let start = Instant::now();
            let result = scan_path_with_simd(&args.input, &panel, args.threads, None, simd)?;
            let elapsed = start.elapsed().as_secs_f64();
            let calls_json = serde_json::to_string(&result.calls)?;
            let checks_json = serde_json::to_string(&result.checks)?;
            let (agreement, qc) = match &baseline {
                None => {
                    baseline = Some((calls_json, checks_json));
                    (
                        "scalar baseline".to_string(),
                        serde_json::to_string(&result.qc)?,
                    )
                }
                Some((reference_calls, reference_checks)) => {
                    if reference_calls == &calls_json && reference_checks == &checks_json {
                        (
                            "100% calls+checks".to_string(),
                            serde_json::to_string(&result.qc)?,
                        )
                    } else {
                        anyhow::bail!("SIMD output mismatch; refusing benchmark success");
                    }
                }
            };
            let bytes = std::fs::metadata(&args.input)?.len();
            println!(
                "scan\t{level:?}\t{bytes}\t{elapsed:.6}\t{:.3}\t{}\t{}\t{}",
                bytes as f64 / elapsed / (1024.0 * 1024.0),
                result.qc.reads,
                agreement,
                result.engine
            );
            if args.repeats == 1 {
                eprintln!("{level:?} qc {qc}");
            }
        }
    }
    Ok(())
}
