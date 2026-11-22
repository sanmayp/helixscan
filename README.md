# HelixScan

HelixScan is a research and education tool for inspecting a fixed panel of genetic variants in FASTQ files. It is **not a medical test**.

## Build

Requires stable Rust and an x86-64 Linux host. CPU vectorization is selected at runtime; release builds do not use `target-cpu=native`. The web assets are vendored and the application runs offline after the panel is built.

```sh
cargo build --release
cargo run -p helixscan-gpu --bin helixscan-gpu-probe
```

## Commands

The scan, serve, panel-builder, simulator, and benchmark commands are being implemented in numbered milestones. `PROGRESS.md` records verified milestone results and limitations.

## Dependencies

Public crates.io dependencies are used for infrastructure, not copied algorithm code. Direct workspace dependencies are: `anyhow` (errors), `clap` (CLI arguments), `crossbeam-channel` (bounded worker queues), `flate2` (gzip input), `pollster` (GPU futures), `rand` (reproducible simulation), `rayon` (parallel simulation), `regex` (panel-term filtering), `serde` and `serde_json` (data formats), `toml` (versioned pharmacogenetic rules), `ureq` (Ensembl HTTPS requests), and `wgpu` (GPU adapter access). Transitive dependencies are locked in `Cargo.lock`. SIMD algorithms and application scanner code remain original project code.

The scanner currently uses a scalar CPU k-mer path. AVX2/AVX-512 and GPU compute remain milestone work; their earlier helper stubs are not claimed as completed acceleration. A GPU adapter probe is available. Reports are research summaries, not clinical pharmacogenetic or ancestry interpretations.

## Safety

Reports describe associations only, never diagnoses or treatment decisions. Research and education only. Talk to a doctor or genetic counselor about any result; never change medicine based on this tool.