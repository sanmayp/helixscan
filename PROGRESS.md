# HelixScan progress

## Environment check (2026-11-21)

- Repository contains the supplied specification and no application code.
- Stable Rust: `rustc 1.100.0`, `cargo 1.100.0`; 12 logical CPUs.
- NVIDIA Quadro P5000, 16 GiB, driver 580.196.07 available through `nvidia-smi`.
- Ensembl REST responds for GRCh38 variation requests.
- Initial repository is on branch `master`.

## M0 — complete

- Created the Cargo workspace and a runtime WebGPU adapter probe.
- Observed Vulkan adapter `Quadro P5000`, NVIDIA 580.196.07; maximum compute workgroup X 1536, maximum invocations 1536, and 65535 compute workgroups per dimension. An unused OpenGL adapter was also listed; the compute backend will prefer Vulkan/NVIDIA.
- Added `.gitignore` for generated reads and build outputs.
- M0 validation: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass.

Next: M1 verified panel builder.

## M1 — complete

- Implemented the verified Ensembl GRCh38 panel builder, seed and phenotype-term searches, allele-frequency capture, APOE rules and pharmacogenetic rule data.
- Committed panel contains 77 variants and three searched traits. One seed (`rs10811661`) was dropped because its expected gene was absent within 50 kb; `data/panel.drop.log` records the reason. Amblyopia remains a searchable trait with no qualifying markers and the required explanation.
- Allele normalization supports SNVs and small insertions/deletions, including exact junction-context reconstruction tests.
- Validation: `cargo fmt --all`, `cargo clippy -p helixscan-panel --all-targets -- -D warnings` and `cargo test -p helixscan-panel` pass.

## M2 — complete

- Implemented reproducible, parallel FASTQ simulation with both strands, read-position-dependent errors, known truth genotypes, and long-read context extension.
- Generated and committed 2.0 MB `testdata/small.fastq` plus its truth file; a separate 181 MB benchmark fixture remains gitignored.
- Simulator chunk determinism, ordering, FASTQ structure, and substitution reproducibility pass unit tests.

## M3 — complete

- Implemented bounded parallel FASTQ reading with reusable buffers, record-tail handling, gzip input, thread-local QC and allele counters.
- Implemented per-allele SNV/indel junction k-mers with strand-aware event quality filtering and one vote per read and variant. Shared allele k-mers are excluded from the exact index.
- Added likelihood genotype calls, depth/GQ, multi-allele selection, hemizygous XY calls, and mitochondrial heteroplasmy.
- Exhaustive FASTQ block boundaries, low-quality variant rejection, both strands, indel voting and simulated truth agreement pass: **100.000% (77/77 variants called; depth 40)**.
- Validation: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass.

## M3b — complete

- Added single-pass marker checks using bounded SRY, UTY, RPS4X, KDM6A and ACTB sequences. Y evidence excludes X-homolog and ACTB control k-mers. Explicit broad thresholds classify simulated XX/XY samples; low or intermediate evidence remains `unclear`.
- Added telomeric read counting (at least seven tandem repeats), length-adjusted reporting and non-age-estimation wording.
- Added five population log-likelihood comparisons from called panel variants, including marker counts, relative scores and a cautious summary. This is limited to available panel markers and is not an ancestry or identity determination.
- Added allele-specific versioned TOML PGx evaluation, insufficient-coverage handling, hemizygous XY dosage and data-derived sensitive-report limitations. Simulator options now generate sex-marker, telomere and per-population-frequency reads.
- Preserved all 77 verified variant records and contexts. The earlier phantom FASTQ record was a test-file race, not a parser defect; fixtures now use unique paths.
- Measured on 20,000-read XX/XY simulation fixtures (seed 71, depth 20): both sex-marker classifications passed. A 0.2 telomeric fraction produced over 500 telomeric reads. AFR frequency draws produced scores for all five groups. Versioned PGx rules and low-coverage failures passed.
- Validation: 24 workspace tests pass, including **100.000% / 77 of 77** original simulated genotype agreement; `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

Next: M4 real AVX2 and AVX-512 encoders, parity tests and benchmark.

## M4 — complete; throughput target remains open

- Implemented runtime-selected scalar, AVX2 assembly, AVX-512 assembly and AVX-512 intrinsics twin encoders. SIMD encoding now feeds reusable panel, X-marker and telomere k-mer scan buffers.
- Encoding/rolling-key parity covers lengths 0–256, arbitrary bytes, lowercase and unaligned slices, real valid keys across N boundaries, multiple reads and randomized valid runs. Assembly is tested against the independent AVX-512 intrinsics helper.
- Full scanner parity is asserted by the benchmark using a persistent scalar baseline: calls and whole-sample checks must match exactly. `small.fastq` retains **77/77 called variants at 100.000% simulated-truth agreement**.
- Measured release benchmark on the 181 MB, 600,000-read fixture: 256 MiB encoding = **511.809 scalar / 511.517 AVX2 / 467.803 MiB/s AVX-512**. Warm scans (repeats = 2) measured **95.707 / 91.106 / 154.698 MiB/s**, respectively; full JSON equality passed for all three. These are measured scanner results, not encoding-only comparisons.
- Validation: formatting, strict workspace Clippy and **28 workspace tests** pass. The SIMD encoder is in the actual scanner, but this fixture does **not meet the 1.5 GB/s SPEC target**; optimization remains required. M7 will report both measurements and limitations.

Next: M5 generic NVIDIA Vulkan compute with measured CPU fallback, then app/UI and final benchmarking.
