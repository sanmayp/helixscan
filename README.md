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

Public Rust crates are used for infrastructure (not copied algorithm code). The workspace currently uses `wgpu`, `anyhow`, and `pollster`. The final list will accompany the completed project.

## Safety

Reports describe associations only, never diagnoses or treatment decisions. Research and education only. Talk to a doctor or genetic counselor about any result; never change medicine based on this tool.