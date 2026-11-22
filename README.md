# HelixScan

HelixScan is an offline research and education tool for inspecting a fixed panel of genetic variants in FASTQ files. It is **not a medical test**. All matching, genotyping and sample checks are performed locally; web assets are vendored.

## How to run

Build with stable Rust on x86-64 Linux. CPU SIMD support is selected at runtime; do not use `target-cpu=native`.

```sh
cargo build --release

# Create a JSON research report. CPU is explicit and needs no GPU.
target/release/helixscan scan testdata/small.fastq \
  --panel data/panel.json --backend cpu --threads 4 --out report.json

# Start the locally bound web UI and open http://127.0.0.1:8780/.
target/release/helixscan serve --port 8780 --input-dir testdata --threads 4
```

In the UI, pick a FASTQ file from the configured input directory (or upload one), choose the thread count, and select **CPU · runtime SIMD**. Press **Start scan** for live SSE progress, live votes and the animated helix, then **Open report**. The saved report supports JSON export and browser print-to-PDF. GPU is explicit opt-in in the UI; web `auto` means CPU with runtime SIMD.

For a different fixture directory, pass `--input-dir /path/to/folder`. To allow network access to the UI, explicitly use `--bind 0.0.0.0` and protect the service appropriately; the default is `127.0.0.1`. `--store-dir` sets where report JSON is saved. FASTQ input must be uncompressed or gzip-compressed.

The scan CLI also accepts `--backend auto`, `--backend gpu`, `--gpu-index` and `--gpu-mem-mb`. Auto benchmarks the GPU candidate against CPU and retains CPU unless the entire biological result matches and GPU is faster. A forced GPU error is not silently accepted. See `bench/RESULTS.md` and `PROGRESS.md` for measured behavior and limits.

A [60-second demo](docs/DEMO.md) scans a 1.27 GB synthetic FASTQ and automatically opens its report. The fixtures under `bench/data/` are gitignored and must be generated or retained locally; they are not included in a fresh Git checkout.

## Build and verify

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
node --check web/app.js
node --check web/helix.js
```

The committed `testdata/small.fastq` and its truth JSON support quick correctness tests. Generate a reproducible larger fixture (about 1 GB at two million 150-base reads) using:

```sh
cargo run --release -p helixscan-sim -- \
  --panel data/panel.json --out bench/data/large.fastq \
  --reads 2000000 --len 150 --depth 30 --seed 71 \
  --truth bench/data/large.truth.json
```

The committed panel is `data/panel.json` (77 verified variants). Rebuild it with `cargo run --release -p helixscan-panel -- build --seed data/panel.seed.tsv --out data/panel.json --drop-log data/panel.drop.log`; rebuilding needs network access, while scanning and serving do not. A marker dropped during verified construction is explained in `data/panel.drop.log`.

## Benchmarks

```sh
# Scalar, AVX2, AVX-512, forced-GPU and measured auto selection.
target/release/helixscan bench bench/data/large.fastq --threads 4 --repeats 1

# Full-scanner SIMD parity and separate encoding measurements.
target/release/helixscan-kmer-bench \
  --input bench/data/large.fastq --threads 4 --repeats 1
```

`helixscan bench` uses the runtime default GPU configuration. Use `--gpu-index` on `helixscan scan` to select a runtime-discovered adapter. On this NVIDIA host, benchmarks also used `VK_ICD_FILENAMES=/home/genescan/.config/vulkan/nvidia_icd.json`; this is a host-specific JSON file, not a portable path. No GPU result is accepted unless its biological calls match CPU.

`bench/RESULTS.md` reports real wall-clock measurements, hardware, commands, fixture details and unmet targets. Reported MiB/s is end-to-end FASTQ bytes divided by elapsed scan seconds and 1,048,576; encoding-only throughput is labeled separately.

## Dependencies

Public crates.io dependencies provide infrastructure, not copied scanner algorithms. Direct workspace dependencies are:
- `anyhow` (errors), `clap` (CLI), `serde`, `serde_json` and `toml` (data formats).
- `crossbeam-channel`, `rayon` (bounded work and simulation parallelism), `flate2` (gzip), `rand` (reproducible simulation).
- `regex`, `ureq` (panel selection and Ensembl HTTPS requests).
- `axum`, `tokio`, `tower`, `http-body-util`, `async-stream`, `uuid` (local web server, async SSE and tests).
- `wgpu`, `pollster` (GPU adapter and generic Vulkan/WGSL candidate matching).

Three.js is vendored in `web/vendor/` with its license. Transitive Rust dependencies are pinned in `Cargo.lock`. SIMD encoding, FASTQ matching and report/application logic are project code.

## Safety

Reports describe associations only, never diagnoses or treatment decisions. Research and education only. Talk to a doctor or genetic counselor about any result; never change medicine based on this tool. PGx calls are incomplete and cannot determine CYP2D6 or full HLA type. The sample similarity check uses only available panel markers and is not ancestry or identity determination. A negative result does not exclude unexamined variants.
