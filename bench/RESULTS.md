# Measured results

## H2 four-worker CPU optimization (2026-11-22, `perf-h2`)

`demo-1gb.fastq` is 1,273,289,704 bytes and 4,000,000 reads. All rows are sequential four-worker `CPU Avx512` scans with report scan time, external process wall time, and `/proc/loadavg` at start/end. GiB/s is input FASTQ bytes divided by scan seconds and 2^30. They are real runs on the shared host, not isolated controlled-cache runs. Raw reports, logs, metrics and the saved baseline binary are in untracked `bench/h2/`.

All runs call 77/77 truth variants, report 100% panel coverage, and retain byte-identical biological JSON after excluding only scan elapsed time and engine metadata. Canonical normalized biological JSON SHA-256 is `6d1a625f9962a82c49fe73295452e0f58a44ac76773e423272630ae727a2c466`.

| Stage | Run | Scan s | Wall s | GiB/s | Load at start | Load at end |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Baseline `aa4237e` | 1 (contended) | 13.755564989 | 14.680 | 0.086208 | 4.93 2.31 0.93 3/910 319502 | 5.07 2.47 1.01 1/892 319716 |
| Baseline `aa4237e` | 2 | 11.939225765 | 12.680 | 0.099323 | 5.07 2.47 1.01 1/892 319722 | 4.98 2.54 1.04 1/892 319729 |
| Baseline `aa4237e` | 3 | 11.908009032 | 12.650 | 0.099584 | 4.66 2.51 1.04 1/892 319735 | 4.56 2.56 1.08 2/887 319744 |
| Reader/gzip only (unreleased) | 1 | 12.339903236 | 13.130 | 0.096098 | 3.66 2.62 1.92 2/942 345572 | 3.72 2.66 1.94 1/938 345612 |
| Reader/gzip only (unreleased) | 2 | 12.683535205 | 13.460 | 0.093495 | 3.72 2.66 1.94 2/938 345616 | 3.78 2.73 1.97 1/945 345845 |
| Reader/gzip only (unreleased) | 3 | 12.379069056 | 13.150 | 0.095794 | 3.78 2.73 1.97 2/945 345849 | 3.81 2.77 1.99 1/940 345876 |
| Exact first-gain candidate | 1 | 4.265771879 | 5.010 | 0.277990 | 3.91 4.63 3.14 2/918 368060 | 3.92 4.62 3.14 1/914 368067 |
| Exact first-gain candidate | 2 | 4.272545412 | 5.020 | 0.277550 | 3.92 4.62 3.14 3/914 368071 | 4.00 4.63 3.15 1/914 368078 |
| Exact first-gain candidate | 3 | 4.336093543 | 5.080 | 0.273482 | 4.00 4.63 3.15 2/914 368082 | 4.00 4.62 3.16 2/916 368091 |

The exact first-gain candidate contains the reader/gzip fixes, an exact marker prefix/filter table, rolling telomere detection and SIMD FASTQ newline search. All three final runs are about 2.8 times the best baseline rate; the reader-only rows were not an independent gain. The consolidated FASTQ parser attempt was reverted to its tested tail-preserving boundary finder after its regression, with SIMD newline search used in both newline loops. This table describes the exact candidate selected for the first gain commit (`dbf073d`).

### Marker rejection bitmap (`07b5d63`)

The marker filter now uses 2^21 hash-derived bits (256 KiB), instead of a 16 MiB byte-per-prefix table. Exact full-key probing still resolves every positive. The original 21-bit prefix of each key is deliberately preserved in the filter bit selection, even though the exact-table slot uses the wider mixed hash; a focused collision test confirms that different keys sharing a filter bit retain their exact marker semantics.

| Stage | Run | Scan s | Wall s | GiB/s | Load at start | Load at end |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Marker bitmap `07b5d63` | 1 | 2.607223924 | 3.360 | 0.454830 | 5.56 3.44 2.40 2/900 403387 | 5.27 3.42 2.39 2/900 403395 |
| Marker bitmap `07b5d63` | 2 | 2.561744960 | 3.320 | 0.462905 | 5.27 3.42 2.39 2/900 403398 | 5.25 3.45 2.41 2/899 403408 |
| Marker bitmap `07b5d63` | 3 | 2.571521482 | 3.330 | 0.461145 | 5.25 3.45 2.41 2/899 403411 | 5.25 3.45 2.41 2/899 403419 |

Each run called 77/77 variants with 100% truth agreement and produced the canonical biological SHA-256 above. Validation included all nine GPU tests serially, all 47 workspace tests serially, a subsequent ordinary default-thread `cargo test --workspace`, and strict workspace Clippy. One earlier parallel debug GPU test process exited with SIGSEGV; its cause was not established. The isolated serial GPU run, serial workspace run and later ordinary workspace run all passed, so the SIGSEGV is not presented as a resolved defect or as evidence of a scanner failure.

### Fused CPU panel/marker rejection (next gain)

A worker-local combined 256 KiB bitmap starts as a copy of the panel filter and adds the marker filter bits. One lookup rejects the common negative case for both indexes. Positive cases still run the original exact marker lookup and panel full-key probing, including the X-homolog veto. Standalone `MarkerScanner::new`, `read_hits_with` and GPU uploads retain their previous behavior. This changes only CPU fused traversal, not evidence or voting rules.

Three sequential baseline/candidate pairs used the same four-worker AVX-512 fixture and command. Baseline was the saved `07b5d63` release binary; candidate was the fused-filter release. The candidate preceded no baseline and all baselines preceded their candidate in each pair, so these are sequential paired measurements rather than randomized or isolated-cache claims.

| Pair | Binary | Scan s | Wall s | GiB/s | Load at start | Load at end |
| ---: | --- | ---: | ---: | ---: | --- | --- |
| 1 | `07b5d63` | 2.588863876 | 3.330 | 0.458056 | 5.73 3.82 2.80 3/931 438344 | 5.73 3.82 2.80 1/931 438352 |
| 1 | Fused filter | 2.294094529 | 3.040 | 0.516911 | 5.73 3.82 2.80 1/931 438355 | 5.59 3.83 2.81 1/930 438397 |
| 2 | `07b5d63` | 2.571284578 | 3.310 | 0.461187 | 5.59 3.83 2.81 1/930 438400 | 5.22 3.78 2.80 1/930 438408 |
| 2 | Fused filter | 2.334240015 | 3.070 | 0.508021 | 5.22 3.78 2.80 1/930 438411 | 5.22 3.78 2.80 1/930 438419 |
| 3 | `07b5d63` | 2.556325014 | 3.300 | 0.463886 | 5.22 3.78 2.80 1/930 438422 | 5.12 3.78 2.80 1/930 438430 |
| 3 | Fused filter | 2.326372840 | 3.060 | 0.509739 | 5.12 3.78 2.80 1/930 438433 | 5.12 3.78 2.80 1/930 438441 |

Per-pair scan-time reductions are **11.386%, 9.219%, and 8.995%** (9.872% comparing mean scan time: 2.572158 to 2.318236 s). All six runs called 77/77 variants, reported 100% truth agreement, and produced byte-identical normalized biological JSON. The earlier unpaired candidate runs were 2.496468910, 2.577705445 and 2.422763059 s under one-minute load averages 5.65–5.68; the paired runs above are the retention evidence.

The focused post-marker-bitmap Callgrind profile (`callgrind-marker-gain.out`, `profile-marker-gain.txt`) ran the real-panel small fixture serially and recorded 449,213,799 Ir. It still showed per-read `RandomState` hashing at 56,587,258 Ir (12.60%) and `scan_read_with_markers` closure cost at 45,400,134 Ir (10.11%). This is instruction cost rather than a direct prediction of wall time. Reusable vote scratch, dense panel entries, packed marker entries, direct marker hash arithmetic, and reuse of the fused hash in both exact tables were measured and reverted because they were slower or showed no independent gain. A smaller panel bitmap broke GPU parity and was also reverted; GPU hash/index compatibility was not changed.

Required I/O changes: gzip input uses `flate2::bufread::MultiGzDecoder` over a 1 MiB `BufReader`; core and GPU paths pass the already-open file to it; reads target reserved spare capacity rather than a repeatedly zero-filled vector. FASTQ record boundaries, truncation checks and `@` quality handling remain covered.

### H2 profile

`perf` was unavailable on the host. Valgrind 3.26.0 Callgrind ran `tests::real_panel_simulated_truth_agreement` on the shared 2,071,530-byte `testdata/small.fastq` fixture: wall 13.25 s, user 11.93 s, system 1.29 s; load `2.23 2.26 1.19 2/889 325599` to `1.96 2.20 1.18 1/903 325810`. This is an instruction-cost profile of the small fixture, not the 1 GB timing above. It recorded 932,709,250 instruction references. Leading measured costs were:

| Function / operation | Ir | % of total |
| --- | ---: | ---: |
| `RandomState::hash_one::<&usize>` | 225,272,047 | 24.152441% |
| `MarkerScanner::add_telomere` | 174,683,120 | 18.728572% |
| SipHash `Hasher::write` | 136,264,894 | 14.609579% |
| `observe_marker_key` | 64,713,726 | 6.938253% |
| `HashSet<u64>` construction | 51,311,921 | 5.501384% |

The exact marker cache uses a prefix rejection table followed by full-key open addressing. It retains the public marker sets and their behavior; no probabilistic membership verdict is used. Telomere detection uses one rolling 42-base comparison in either motif orientation, with differential tests covering random ambiguous input, all repeat counts through nine, and motif insertions at every tested offset.

H2 candidate validation: release core/GPU tests (**34 tests**, including whole-result GPU parity), randomized telomere/reference equivalence, and strict scanner/GPU Clippy (`-D warnings`) passed. The final full-workspace test and gzip integration checks remain tracked in `PROGRESS.md`.

All measurements use the repository's release build on the Rocky Linux 9.8 host: Intel Xeon W-2135 (6 cores / 12 threads, AVX-512), NVIDIA Quadro P5000 16 GiB, driver 580.196.07. No benchmark claims the 1.5 GB/s target has been met.

## SPEED worker scaling, parser gain and gzip (2026-11-22)

The host has six physical cores and twelve logical CPUs. These sequential `ca48199` scans use `demo-1gb.fastq` (1,273,289,704 bytes, 4,000,000 reads). All normalized biological JSON matches SHA-256 `6d1a625f9962a82c49fe73295452e0f58a44ac76773e423272630ae727a2c466`. Every run calls 77/77 variants; each called allele is present in its simulated truth allele set. This allele-containment check handles haploid `G` versus truth `G/G` and reversed allele order without changing the biological result. Full raw reports, logs and load records are in untracked `bench/r3/`.

| Workers | Run | Scan s | Uncompressed GiB/s | Load at start | Load at end |
| ---: | ---: | ---: | ---: | --- | --- |
| 1 | 1 | 9.372805394 | 0.126544 | 0.75 0.94 1.50 1/935 456018 | 0.98 0.97 1.50 3/974 456032 |
| 1 | 2 | 9.500617271 | 0.124840 | 0.98 0.97 1.50 3/974 456034 | 1.01 0.98 1.50 1/948 456046 |
| 1 | 3 (contended) | 10.913590665 | 0.108685 | 1.01 0.98 1.50 1/948 456047 | 2.22 1.75 2.05 13/958 456061 |
| 4 | 1 (contended) | 4.566703195 | 0.259671 | 2.22 1.75 2.05 13/958 456064 | 3.32 1.99 2.13 5/950 456213 |
| 4 | 2 | 2.568206718 | 0.461739 | 3.32 1.99 2.13 5/950 456214 | 3.32 1.99 2.13 2/945 456221 |
| 4 | 3 | 2.499966398 | 0.474343 | 3.32 1.99 2.13 2/945 456222 | 3.54 2.06 2.15 2/934 456247 |
| 8 | 1 | 1.788100908 | 0.663185 | 3.54 2.06 2.15 2/934 456248 | 3.41 2.06 2.15 1/946 456281 |
| 8 | 2 | 1.573864860 | 0.753459 | 3.41 2.06 2.15 1/946 456282 | 3.41 2.06 2.15 1/946 456297 |
| 8 | 3 | 1.565732852 | 0.757372 | 3.41 2.06 2.15 1/946 456298 | 3.41 2.06 2.15 1/946 456315 |
| 11 | 1 | 1.434355507 | 0.826873 | 3.41 2.06 2.15 1/946 456316 | 3.22 2.04 2.14 1/946 456335 |
| 11 | 2 | 1.420828474 | 0.834614 | 3.22 2.04 2.14 1/946 456335 | 3.22 2.04 2.14 1/946 456355 |
| 11 | 3 | 1.428142311 | 0.830339 | 3.22 2.04 2.14 1/946 456356 | 3.92 2.20 2.20 1/946 456374 |

Quieter rows are approximately 9.5 s at one worker, 2.50 s at four, 1.57 s at eight and 1.42 s at eleven. Extra workers have diminishing returns after eight; serial FASTQ parsing/boundary discovery was the principal identified bottleneck. This shared-host matrix does not demonstrate I/O exhaustion or controlled cold/warm behavior. Contended runs are retained, not presented as software gains.

### Single-pass FASTQ boundary and offset discovery

The candidate discovers complete four-line boundaries and sequence/quality offsets together, instead of scanning each block twice. Eleven-worker sequential baseline/candidate pairs used the saved exact `ca48199` release executable, then the candidate. Every pair has byte-identical normalized biological JSON.

| Run | Baseline scan s | Candidate scan s | Load before baseline | Load after candidate |
| ---: | ---: | ---: | --- | --- |
| 1 | 1.418409149 | 1.354756060 | 1.44 1.42 1.83 1/913 458145 | 2.12 1.56 1.88 1/912 458174 |
| 2 | 1.414942864 | 1.363267533 | 2.12 1.56 1.88 1/912 458176 | 2.83 1.72 1.93 1/912 458205 |
| 3 | 1.411356060 | 1.397246439 | 2.83 1.72 1.93 1/912 458207 | 3.25 1.82 1.96 2/927 458253 |

Mean scan time decreased from 1.414902691 s to 1.371756678 s: **3.05% less scan time**. Baseline end/start loads were respectively `1.44 1.42 1.83 1/913 458159` / `2.12 1.56 1.88 1/912 458176`, `2.12 1.56 1.88 1/912 458190` / `2.83 1.72 1.93 1/912 458207`, and `2.83 1.72 1.93 1/912 458223` / `3.25 1.82 1.96 2/927 458224`. Candidate start loads were `1.44 1.42 1.83 1/913 458160`, `2.12 1.56 1.88 1/912 458191`, and `2.83 1.72 1.93 1/912 458224`; candidate end loads equal the next baseline start loads in the table. External candidate wall times were 2.105, 2.111 and 2.154 s; baseline wall times were 2.170, 2.162 and 2.156 s. These are paired sequential measurements, not isolated cache results. The standalone full-record parser is retained for non-block validation.

### gzip input

The gzip fixture is `bench/data/demo-1gb.fastq.gz`, generated with `gzip -1`, observed size 642 MiB, SHA-256 `bc945a6b790ef9e8002bf401da21a18edc33b8d84c50c0e07daa0a372921be3b`. The data-directory symlink belongs to `/home/genescan/helixscan`; its `.gitignore:11` rule `/bench/data/` ignores the fixture. The speed worktree reports `bench/r3/`, `bench/speed-r3.py` and `bench/data` as untracked, not ignored, so benchmark artifacts were not staged.

| Run | Scan s | Uncompressed GiB/s | Load at start | Load at end |
| ---: | ---: | ---: | --- | --- |
| 1 | 4.289758190 | 0.276435 | 2.99 1.79 1.95 3/950 458664 | 3.15 1.84 1.96 4/1092 458905 |
| 2 | 3.470594549 | 0.341683 | 3.15 1.84 1.96 4/1092 458907 | 3.30 1.90 1.98 2/1074 458937 |
| 3 | 3.705236073 | 0.320045 | 3.30 1.90 1.98 3/1074 458939 | 3.35 1.93 1.99 1/1074 459022 |

All gzip outputs match uncompressed biological output byte-for-byte. A separate GNU `gzip -cd ... > /dev/null` check took 8.40 s wall and 8.30 s user CPU on the idle host. The scanner's zlib-rs decode, parse and scan finishes faster, so zlib-rs is retained; single-thread decompression remains a clear bottleneck, not an extra worker-count gain. No I/O exhaustion or parallel decompression optimization is claimed.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace` (**48 tests**) and `cargo clippy --workspace --all-targets -- -D warnings` pass. The new gzip test compares offsets and batch bytes against uncompressed input at every chunk boundary for two records. Earlier timing-wrapper failures and test-only compilation errors were fixed; they are not scanner correctness failures. Final biological output equivalence includes only biological fields, excluding scan elapsed time and engine metadata.

### Decoder-thread queue and libdeflate: no retained gain

Subsequent gzip experiments used the exact `22ca2ad` saved release executable as baseline, 11 scanner workers and `bench/data/demo-1gb.fastq.gz`. Each candidate has three alternating baseline/candidate pairs; all biological JSON pairs are byte-identical with the canonical SHA-256 above. Rate is uncompressed GiB/s. Timings are shared-host measurements, not isolated-cache results.

| Candidate / pair | Baseline scan s | Candidate scan s | Candidate wall s | Load at baseline start | Load at candidate end |
| --- | ---: | ---: | ---: | --- | --- |
| zlib-rs decode thread, two-block queue / 1 | 3.926355510 | 3.392353681 | 4.155 | 1.24 1.30 1.17 2/946 480172 | 1.86 1.44 1.21 1/944 480249 |
| zlib-rs decode thread, two-block queue / 2 | 3.771359181 | 3.836434681 | 4.601 | 1.86 1.44 1.21 1/944 480251 | 1.79 1.43 1.21 1/947 480549 |
| zlib-rs decode thread, two-block queue / 3 | 3.523916961 | 3.854110754 | 4.762 | 1.79 1.43 1.21 1/947 480551 | 2.15 1.52 1.24 2/965 481585 |
| libdeflate 1.26.1, bounded 2 GiB member / 1 | 3.369733731 | 4.440706104 | 5.266 | 3.38 1.94 1.40 1/943 483386 | 3.55 2.03 1.43 1/991 484466 |
| libdeflate 1.26.1, bounded 2 GiB member / 2 | 3.363002119 | 3.624752844 | 4.373 | 3.55 2.03 1.43 1/991 484468 | 3.31 2.02 1.44 1/947 484531 |
| libdeflate 1.26.1, bounded 2 GiB member / 3 | 3.734965243 | 3.667636513 | 4.425 | 3.31 2.02 1.44 1/947 484533 | 4.05 2.22 1.51 1/928 484601 |

The queue's paired scan-time reductions were **13.600%, -1.726%, and -9.370%** (negative means slower). Its mean is 3.694299705 s versus 3.740210551 s baseline: only 1.23% faster and not reproducible. libdeflate's reductions were **-31.782%, -7.783%, and +1.803%**. Its mean is 3.911031820 s versus 3.489233698 s baseline: 12.09% slower. Both candidates and their dependency changes were fully reverted; no gzip source or dependency gain is committed.

A direct zlib-rs decode through `gzip_reader` using 8 MiB output reads processed 1,273,289,704 bytes in **3.13 / 3.03 / 2.97 s wall**, with user/system CPU **3.02/0.09, 2.91/0.11, and 2.86/0.10 s**. Load at run starts was `13.22 5.27 2.70 6/1004 493683`, `12.40 5.24 2.70 2/989 493827`, and `12.40 5.24 2.70 1/976 493973`; end loads were `12.40 5.24 2.70 2/989 493826`, `12.40 5.24 2.70 1/976 493972`, and `11.49 5.17 2.69 1/976 494034`. Thus most gzip scan time is single-core decode. The public decoder already receives 8 MiB output slices, so larger read buffers were not a justified retained change. Local-only probe code was removed after measurement.

The 673,091,091-byte fixture has gzip flags `0x08` (filename), no BGZF `BC` extra subfield, and footer ISIZE 1,273,289,704. It is an ordinary `gzip -1` file, not BGZF. A bounded-buffer libdeflate attempt does not justify unbounded whole-file buffering or loss of streaming/multi-member input behavior. Member-parallel decoding was not applicable to this fixture and was not implemented. Three fresh `22ca2ad` baseline scans measured 3.676491522 / 3.452777867 / 3.361811394 s; full start/end loads for all three and for every pair are in untracked `bench/r4/*-metrics.json`. Other builds and browser work on this shared host contributed to load variance; rejected rows are not relabeled as software improvements.

Validation after restoring production sources: `cargo fmt --all -- --check`, `cargo test --workspace` (**48 tests**), strict workspace Clippy and `git diff --check` pass. Only the measurements and decisions are documented; the committed reader remains `22ca2ad`.

## M7 measurement protocol and limitations

The M7 implementation reuses canonical k-mer keys and worker marker buffers, traverses panel and marker keys together on CPU, and sends per-batch vote deltas to progress observers. Biological genotype, QC, telomere, marker and ancestry rules are unchanged.

M7 scanner measurements are sequential, with no overlapping scanner or benchmark jobs. The first optimized scalar row overlapped a short focused API build/test and is retained as measured, not relabeled isolated. Raw M7 stdout/stderr logs and the phase-resumable runner are in `helixscan-runs/`. MiB/s is FASTQ input bytes divided by elapsed seconds and 1,048,576. SIMD scanner commands are:

```sh
target/release/helixscan-kmer-bench --input bench/data/10gb.fastq \
  --threads 2 --repeats 1 --encoding-mb 16
```

The thread matrix runs at 1, 2, 4, 6 and 11; each complete pass measures scalar, AVX2 and AVX-512. The saved baseline files retain older measurements and are labeled accordingly. `first` and `repeat` denote sequential accesses, not cold/warm cache proof: this unprivileged host cannot reliably evict Linux page cache for these fixtures. Earlier runs may overlap work; they are explicitly marked contended.

The 20 GB fixture is 20,227,006,292 bytes and 63,492,064 reads (`--reads 63492064 --len 150 --depth 30 --seed 71`). Its existing M6-era four-worker AVX-512 run took 326.662166 s (59.052 MiB/s). Its simulated truth agreement is **100.0% (77/77 called variants)**, with minimum requested depth 30 and observed minimum called depth 25. This is an exact reported allele set check; mitochondrial alleles are called with heteroplasmy. The report also contains the sensitive-research limitations.

The 10 GB and 50 GB synthetic fixtures are 9,999,999,884 bytes and 49,999,999,950 bytes respectively. Their high fraction of generated unrelated reads makes throughput slower than the mixed-content M5 fixture; this is a workload difference, not solely a SIMD property. Encoding-only measurements of ~511 MiB/s are not end-to-end scan results.

### M7 short optimized probe

The optimized release was probed on `testdata/medium.fastq`, 189,520,530 bytes and 600,000 reads, 11 workers. Scalar ran 1.014672 and 1.003561 s (178.127/180.100 MiB/s), AVX2 0.998196 and 1.003106 s (181.068/180.181), and AVX-512 1.088982 and 1.063302 s (165.972/169.981). Every repeated SIMD run reported 100% calls+checks parity. Encoding (16 MiB) was 514.550 scalar, 511.663 AVX2, 511.378 AVX-512 MiB/s.

The GPU probe (2,048 MiB cap, forced NVIDIA Vulkan ICD) ran CPU in 1.108039 s, forced GPU in 7.756851 s (28.007 MiB/s), and auto in 7.546277 s. Auto compared the complete biological result, then selected CPU because GPU candidate work took 6.238 s versus CPU 1.117 s. GPU is slower, not a claimed speedup.

## M7 large-fixture results

M7 optimized scaling and 50 GB measurements are recorded below only after their commands finish. Existing measurements follow for provenance; none are substituted for missing optimized runs.

### Optimized 10 GB SIMD scaling

Each cell is `first / repeat` seconds with end-to-end MiB/s in parentheses. Every optimized non-scalar row reports exact calls+checks agreement with its scalar baseline. All completed runs read 31,407,338 reads and report 100% panel coverage. The scalar one-thread rows are retained M6-era baselines; two through 11 threads are the M7 release with the shared traversal and delta progress fixes.

| workers | Scalar | AVX2 | AVX-512 |
| --- | ---: | ---: | ---: |
| 1 | 342.661849 / 340.563840 (27.831 / 28.003) | 340.626565 / 340.658835 (27.998 / 27.995) | 380.962340 / 381.334675 (25.033 / 25.009) |
| 2 | 171.186157 / 171.761882 (55.710 / 55.523) | 175.717466 / 171.775009 (54.273 / 55.519) | 188.383088 / 187.720433 (50.624 / 50.803) |
| 4 | 85.177413 / 85.210590 (111.963 / 111.920) | 85.375627 / 85.222715 (111.703 / 111.904) | 93.606999 / 93.460316 (101.881 / 102.041) |
| 6 | 58.158392 / 58.104915 (163.979 / 164.130) | 58.246763 / 58.248969 (163.730 / 163.724) | 63.824298 / 64.117867 (149.422 / 148.738) |
| 11 | 46.215663 / 46.156638 (206.353 / 206.617) | 46.575779 / 46.339598 (204.758 / 205.801) | 50.903328 / 50.798897 (187.350 / 187.735) |

The M7 AVX-512 row is slower than scalar at all complete large-fixture thread counts despite similar encoding-only throughput. The scan also performs matching, voting, marker checks, telomere checks and QC; encoding-only speed is not representative. The best 10 GB full-scan rate is 206.617 MiB/s (11 scalar workers), far below the unmet 1.5 GB/s target.

### Optimized 50 GB CPU and GPU

| backend | seconds | MiB/s | correctness evidence |
| --- | ---: | ---: | --- |
| Scalar | 424.687723 | 112.279 | baseline, 156,967,113 reads |
| AVX2 | 424.641346 | 112.292 | 100% calls+checks parity |
| AVX-512 | 466.436032 | 102.230 | 100% calls+checks parity |
| Forced GPU + CPU voting/QC | 1781.151874358 | 26.771 | GPU report parity checked separately below |

These are optimized four-thread, first-pass CPU scans with 100% panel coverage and no overlapping scanner benchmark. This fixture has no retained truth file; its SIMD assertions compare backends, not truth. `helixscan-kmer-bench` does not retain the full scalar JSON report, so direct GPU biological parity uses a separate four-worker CPU report queued after the GPU run; it is not a speed comparison.

The saved GPU report identifies `GPU Quadro P5000; driver NVIDIA 580.196.07; core Vulkan WGSL; workgroup 64; card memory 16384 MiB; cap 2048 MiB; CPU votes/QC`. It processed all 156,967,113 reads and called all 77 variants with 100% panel coverage, but was substantially slower than every CPU row. This is a negative GPU performance result, not a claim of acceleration. Its raw output is `helixscan-runs/m7-gpu50.stdout` and its report is retained locally as `helixscan-runs/50gb-gpu-report.json`.

The forced-GPU scan uses four workers and the configuration recorded in that engine string. A separate four-worker CPU report supplies the direct biological comparison. **All 50 GB biological report fields match**, including all 77 genotypes, QC, APOE, traits, PGx and sample checks; only engine descriptions and elapsed times differ. That CPU AVX-512 reference took 462.393096071 s (103.124 MiB/s), corroborating the measured AVX-512 row rather than replacing it. The comparison artifact is retained locally as `helixscan-runs/50gb-parity.json`.

The commands for the saved large-fixture reports are:

```sh
export VK_ICD_FILENAMES=/home/genescan/.config/vulkan/nvidia_icd.json
target/release/helixscan scan bench/data/50gb.fastq --panel data/panel.json \
  --backend gpu --threads 4 --gpu-mem-mb 2048 \
  --out helixscan-runs/50gb-gpu-report.json
# Same fixture and worker count; direct reference for biological parity:
target/release/helixscan scan bench/data/50gb.fastq --panel data/panel.json \
  --backend cpu --threads 4 --out helixscan-runs/50gb-cpu-report.json
```

The separate CPU parity wrapper expired before starting its scan; inspection confirmed no report or active child, after which the standalone command above completed successfully. No GPU or completed SIMD scan was rerun. A second CPU timing pass and an all-thread 50 GB matrix remain outside the measured set; the spec's full cold/warm matrix is therefore not claimed.

### Preserved M6-era 10 GB baseline

An isolated one-worker, two-pass matrix measured scalar 342.661849/340.563840 s (27.831/28.003 MiB/s), AVX2 340.626565/340.658835 s (27.998/27.995), and AVX-512 380.962340/381.334675 s (25.033/25.009). All 31,407,338 reads matched scalar calls/checks. The intentionally stopped two-worker pass completed only scalar 176.475771 s (54.040 MiB/s).

A contended four-worker M6-era pass (not an isolated scaling result) measured scalar 149.212206 s (63.914 MiB/s), AVX2 154.115344 s (61.881), and AVX-512 123.311641 s (77.339). This is a baseline for the M7 short probe, not an apples-to-apples optimization percentage across fixture content.

The preserved forced-GPU 10 GB M6-era result processed 31,407,338 reads and called all 77 variants in 395.973392566 scan seconds (24.084 MiB/s), with CPU voting/QC. Its full report is retained in the ignored `helixscan-runs/10gb-gpu-report.json`.

## M5 backend comparison

Command:

```sh
VK_ICD_FILENAMES=/home/genescan/.config/vulkan/nvidia_icd.json \
  cargo run --release -p helixscan-gpu --bin helixscan-gpu-bench -- \
  --input testdata/medium.fastq --threads 11 --gpu-mem-mb 2048
```

Fixture: `testdata/medium.fastq`, 189,520,530 bytes, 600,000 reads, seed 7. The table reports end-to-end wall time; the auto diagnostic also records candidate-only GPU time. The benchmark asserted identical calls before recording any GPU result.

| Mode | Wall seconds | Result |
| --- | ---: | --- |
| CPU, runtime AVX-512 | 1.109 | retained |
| Forced GPU + CPU voting/QC | 7.944 | calls match |
| Auto GPU + CPU comparison | 7.706 | slower GPU; selected CPU; GPU candidate 6.411 s |

The GPU used core Vulkan WGSL, workgroup 64, 256-read tuned batches and a 2,048 MiB cap. Auto correctly chooses the measured CPU backend for this fixture; this is not a claimed GPU speedup.
