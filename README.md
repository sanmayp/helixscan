# HelixScan

HelixScan is a research and education tool for inspecting a fixed panel of genetic variants in FASTQ files. It is **not a medical test**.

## Build

Requires stable Rust and an x86-64 Linux host. CPU vectorization is selected at runtime; release builds do not use `target-cpu=native`. The web assets are vendored and the application runs offline after the panel is built.

```sh
cargo build --release
cargo run -p helixscan-gpu --bin helixscan-gpu-probe
cargo run --release -p helixscan-gpu --bin helixscan-gpu-bench -- --input testdata/medium.fastq
```

## Commands

The scan, serve, panel-builder, simulator, and benchmark commands are being implemented in numbered milestones. `PROGRESS.md` records verified milestone results and limitations.

## Dependencies

Public crates.io dependencies are used for infrastructure, not copied algorithm code. Direct workspace dependencies are: `anyhow` (errors), `clap` (CLI arguments), `crossbeam-channel` (bounded worker queues), `flate2` (gzip input), `pollster` (GPU futures), `rand` (reproducible simulation), `rayon` (parallel simulation), `regex` (panel-term filtering), `serde` and `serde_json` (data formats), `toml` (versioned pharmacogenetic rules), `ureq` (Ensembl HTTPS requests), and `wgpu` (GPU adapter access). Transitive dependencies are locked in `Cargo.lock`. SIMD algorithms and application scanner code remain original project code.

The core scanner uses runtime-selected scalar, AVX2 and AVX-512 assembly encoding with reusable k-mer buffers. A standalone `helixscan-kmer-bench` checks full-scanner parity and measures real FASTQ throughput. The GPU backend uses vendor-neutral WGSL candidate matching, runtime NVIDIA adapter limits, bounded buffers and measured CPU fallback. Full-result equality is checked before auto backend selection. Reports are research summaries, not clinical pharmacogenetic or similarity interpretations.

## Safety

Reports describe associations only, never diagnoses or treatment decisions. Research and education only. Talk to a doctor or genetic counselor about any result; never change medicine based on this tool.