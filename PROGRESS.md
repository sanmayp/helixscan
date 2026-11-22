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

## M5 — complete; GPU candidate is slower, so auto selects CPU

- Implemented generic NVIDIA Vulkan/WGSL k-mer candidate matching using runtime adapter limits, no vendor features, bounded hit output, two pending dispatches, and conservative buffers capped at the requested GPU-memory limit.
- CPU quality voting, genotype calls, QC, marker checks, and report inputs are shared with the GPU path. Raw hit parity covers strands, ambiguous bases, long reads, exact-table probing, multi-read splitting, calibration, and hit-capacity exhaustion.
- Auto mode validates the complete biological result before selecting a backend; a mismatch is never silently treated as a timing result. Forced GPU errors and fallback reasons are preserved. Tuning cache, memory policy, and adapter selection are unit tested.
- Fixed the calibration generator's four-base indexing and narrowed report parity to biological fields, retaining the intentionally distinct CPU/GPU engine metadata. Full report content is otherwise identical.
- Release benchmark, 181 MB / 600,000-read fixture, 11 threads, Quadro P5000, driver 580.196.07, workgroup 64, 256-read tuned batches, 2,048 MiB cap: CPU **1.109 s**; forced GPU **7.944 s wall clock**; GPU candidate **6.411 s**. Auto selected CPU after matching the complete result. GPU-only candidate enumeration did not improve end-to-end throughput.
- Validation: formatting, strict workspace Clippy, and **28 workspace tests** pass. `helixscan-gpu-bench` records real CPU/forced-GPU/auto comparisons.

Next: M6 app CLI, SSE scan workflow, vendored 3D scan UI, report UI and small-screen layout.

## M6 — web workflow verified; phone visual capture blocked

- Added the `scan`, `serve`, and `bench` CLI commands, offline Axum web server, safe configured-file selection/upload, scan snapshots, Server-Sent Events, saved reports, JSON export and printable HTML report routes.
- Added the dark scan/report UI, live telemetry, vote/depth bars, trait and ancestry summaries, gene-depth chart and vendored Three.js double helix. Its instanced strands, scan ring and progress-linked motion use scan values, with reduced-motion handling. No external web CDN is required.
- Fixed concurrent vote accounting to add each worker's totals; the progress unit test passes. Reused immutable panel/marker indexes at server startup to avoid setup time obscuring scan progress.
- Fixed fast-scan SSE completion delivery and loading a saved report via its HTML route. Web `auto` deliberately maps to runtime-SIMD CPU; explicit GPU retains verification and fallback reporting. CLI auto benchmarking remains separate.
- Live curl verification on release-build server, report `71975189b57441e9b214abdbe2647234`: all page assets and panel/files/telemetry endpoints returned 200; path traversal and zero threads returned 400. The medium fixture emitted **10 nonzero intermediate running snapshots** and completed with **189,520,530 bytes, 600,000 reads, 90,000,000 bases, 77 calls and 100% panel coverage** on four threads. Saved snapshot/events/report HTML and current JSON/HTML report endpoints returned 200.
- Full validation after the SSE fix: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (two consecutive complete runs), `cargo build --release`, JavaScript syntax checks and `git diff --check` pass. **39 workspace tests** pass (2 app integration, 19 core unit, 1 progress integration, 9 GPU, 4 panel, and 5 simulator unit tests). Thread-specific marker-test fixture paths prevent parallel tests from deleting each other's input.
- Small-screen media queries, 44 px controls, reduced helix size and contained tables are implemented. Browser screenshot fallback did **not verify 360 px visual rendering**: Firefox captures were 1366x1346 and no supported Chromium was installed. HTTP/SSE and source-level responsive checks are the M6 verification of record; no claim of a visual phone pass is made.

Next: M7 measured benchmarks, execution instructions and the 60-second demo.

## M7 — complete; performance target and visual phone verification remain open

- Finished optimized scanner work: allocation-free k-mer visitation, shared CPU panel/marker traversal, filtered voting without per-variant hit-vector copies, reusable worker buffers and delta-based CPU/GPU progress updates. X-homolog veto, telomere, genotype, QC, ancestry, PGx and CPU GPU-voting semantics are preserved.
- Fixed an M6 persistence defect found during M7 verification: `/api/scans/<id>/report.json` now reads a valid saved report before requiring an in-memory job. Invalid saved JSON returns HTTP 422. The new restart regression uses the real report builder and normal no-call genotypes; the 20 GB CLI report HTML, snapshot, SSE replay and JSON routes were also live-verified after wrapping it without rescanning or changing its values.
- Replaced the provisional 60-second promise with an accurate guided presentation: the 20 GB foreground scan previews live evidence, then a separately completed report is opened. The recorded scan took 326.662166 seconds. README now documents scan/serve/benchmark commands, local operation, fixture generation, GPU behavior and validation.
- Recorded the release 10 GB SIMD matrix for first/repeat accesses: best scalar 11-worker rate **206.617 MiB/s**, best AVX2 **205.801 MiB/s**, best AVX-512 **187.735 MiB/s**. All optimized non-scalar rows agree with scalar calls/sample checks. The one-worker rows and older GPU/contended results retain their M6 provenance in `bench/RESULTS.md`.
- Optimized four-worker, first-pass 50 GB results: scalar **424.687723 s / 112.279 MiB/s**, AVX2 **424.641346 s / 112.292 MiB/s**, AVX-512 **466.436032 s / 102.230 MiB/s**, with exact SIMD calls+checks parity. Forced Quadro P5000 Vulkan took **1781.151874358 s / 26.771 MiB/s** at a 2,048 MiB cap. All 50 GB GPU/CPU biological report fields match, excluding only engine and elapsed-time metadata; all 156,967,113 reads, 77 calls and 100% panel coverage were verified. The CPU parity reference took 462.393096071 s.
- The **1.5 GB/s CPU target remains unmet**, and the GPU is not an acceleration result. Encoding-only ~511 MiB/s is not end-to-end throughput. Sequential first/repeat runs are not claimed as controlled cold/warm-cache experiments. A second 50 GB timing pass and complete 50 GB thread matrix were not run. The existing 360 px browser visual verification remains unresolved; HTTP/SSE route checks are not visual evidence.
- Final validation after all scans passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --release`, `cargo test --workspace` (**45 tests**), `node --check web/app.js`, `node --check web/helix.js`, `sh -n helixscan-runs/run-m7-benchmarks.sh` and `git diff --check`.

READY: M0–M7 are committed with reproducible offline scanning, local web/report workflow, measured backend results and documented limitations above.

## H1 — complete; 60-second demo saved-report rendering verified
- The four-worker, 1,273,289,704-byte simulated FASTQ demo scan measured **12.092 seconds** with **77/77 calls and 100% panel coverage**; fixed blank saved-report rendering and verified real persisted-report values in headless Chromium, with `docs/DEMO.md` commands and 60-second shot list.
## H2 — scanner gains committed; further CPU tuning remains open

- Scoped changes to FASTQ reading, scanners, marker lookup, telomere traversal and scanner tests. No genotype, panel/data, simulator or web file was edited. The unfinished marker-construction rewrite was dropped after profiling identified construction rather than the recurring scan as its principal cost.
- Four-worker baseline and first-candidate timings on `demo-1gb.fastq` are in `bench/RESULTS.md`, each repeated three times with load averages and biological-output SHA-256. Exact first-candidate rates are **0.277990 / 0.277550 / 0.273482 GiB/s** vs best baseline **0.099584 GiB/s**. All calls and biological JSON are identical. The first gain is committed as `dbf073d`.
- Added the required gzip decoder/buffering, removed the extra gzip file open from both scanner backends, and read into reserved spare capacity rather than zero-filled reads. `memchr` accelerates newline scans while the validated record-boundary/tail behavior is retained. Gzip integration checks are included in workspace tests.
- Replaced hot-path marker SipHash lookups with immutable full-key open addressing plus prefix rejection; exact public marker sets are unchanged. Telomere detection uses a single 42-base rolling window instead of twelve whole-read restart passes, with reference-equivalence tests.
- `07b5d63` shrinks the exact marker rejection filter from 16 MiB of bytes to a 256 KiB hash-derived bitmap. Full-key probes still resolve every positive. Three scans measured **2.607223924 / 2.561744960 / 2.571521482 s** (walls 3.36 / 3.32 / 3.33 s), with identical biological JSON, 77/77 calls and complete start/end load averages in `bench/RESULTS.md`.
- The next real gain uses a worker-local union of the panel and marker rejection bitmaps on fused CPU traversal. One 256 KiB bitmap lookup rejects negatives for both; original exact marker and panel lookups still resolve positives. GPU and standalone marker behavior stay unchanged. Paired candidate scans measured **2.294094529 / 2.334240015 / 2.326372840 s** (walls 3.04 / 3.07 / 3.06 s), versus paired `07b5d63` baselines of **2.588863876 / 2.571284578 / 2.556325014 s**. Per-pair improvement is **11.386% / 9.219% / 8.995%**; biological SHA-256 remains `6d1a625f9962a82c49fe73295452e0f58a44ac76773e423272630ae727a2c466`.
- Whole-fixture Callgrind completed. A focused post-marker-bitmap Callgrind run recorded 449,213,799 Ir; its profile identified the fused read closure and per-read hash work for the following experiments. Callgrind measures instruction cost, not wall-time gains by itself.
- Rejected and fully reverted experiments: reusable vote scratch/single-variant voting (4.237–4.889 s before the marker gain), dense panel entries (5.31–5.44 s), and a smaller panel bitmap that failed two GPU parity tests. After the marker gain, direct hash arithmetic/OnceLock fast path (2.615–2.671 s), packed marker table entries (2.672–2.732 s), and fused hash reuse (2.409–2.695 s) did not provide separate reproducible gains. Raw measurements are retained under untracked `bench/h2/`.
- Marker-bitmap validation: serial GPU tests passed 9/9; serial workspace tests passed all 47; strict workspace Clippy passed; a subsequent ordinary default-thread `cargo test --workspace` passed. An earlier parallel debug GPU test process exited with SIGSEGV, cause unestablished; this is recorded as a caveat, not claimed fixed. Isolated and later normal runs passed.
- Fused-filter validation: serial `cargo test --workspace -- --test-threads=1` and ordinary default-thread `cargo test --workspace` each passed all **47** tests. Strict `cargo clippy --workspace --all-targets -- -D warnings`, release build and `git diff --check` passed; the rebuilt release was byte-identical to the paired candidate binary. The 1.5 GiB/s target remains unmet; best paired candidate throughput is **0.516911 GiB/s**, and H2 is not declared globally finished.
