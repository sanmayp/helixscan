# Measured results

All measurements use the repository's release build on the Rocky Linux 9.8 host: Intel Xeon W-2135 (6 cores / 12 threads, AVX-512), NVIDIA Quadro P5000 16 GiB, driver 580.196.07. No benchmark claims the 1.5 GB/s target has been met.

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
