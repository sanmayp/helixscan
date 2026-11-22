# 60-second research demo

This demo scans a **real, synthetic 1.27 GB FASTQ in about 12 seconds**, while the local web UI shows the animated double helix and live counters. It automatically opens that same scan's saved report at the end. No pre-recorded progress, previously completed report, network service or human genome is involved.

## Measured result

On 2026-11-22, this repository's release build on its Xeon W-2135 host measured:

| item | result |
| --- | --- |
| fixture | 1,273,289,704 bytes (1.27 GB / 1.19 GiB), 4,000,000 reads, 150 bases/read |
| engine | CPU Avx512, 4 worker threads |
| **real scan time** | **12.092248679 seconds (reported as 12.092 s below)** |
| throughput | 100.420 MiB/s end-to-end FASTQ bytes / scan seconds |
| report | 4,000,000 reads, 77/77 called variants, 100% panel coverage |

This is scanner elapsed time from `target/release/helixscan scan`, not a universal speed promise. Fixture generation, release compilation and opening the initial page happen before recording. On this host that leaves about 48 seconds for the explanation and report tour within the minute. Do not claim this replaces the unmet 1.5 GB/s SPEC target; see `bench/RESULTS.md`.

## Prepare before recording

Run all commands from the repository root.

### 1. Build and generate the fixture

```sh
cargo build --release
mkdir -p bench/data
target/release/helixscan-sim \
  --panel data/panel.json --out bench/data/demo-1gb.fastq \
  --reads 4000000 --len 150 --depth 30 --seed 71 \
  --sex XY --telomere 0.02 \
  --truth bench/data/demo-1gb.truth.json
stat --printf='%s bytes\n' bench/data/demo-1gb.fastq
```

`stat` must print `1273289704 bytes` for the fixture measured here. It is simulated teaching data, not a human genome. The FASTQ and truth file are gitignored; retain them locally or regenerate before the demo.

### 2. Reproduce the timing (optional)

```sh
target/release/helixscan scan bench/data/demo-1gb.fastq \
  --panel data/panel.json --backend cpu --threads 4 \
  --out helixscan-runs/demo-1gb-timing.json
python3 - <<'PY'
import json
r = json.load(open('helixscan-runs/demo-1gb-timing.json'))
print(f"Scan: {r['elapsed_seconds']:.3f} s; {r['engine']}")
print(f"Calls: {sum(v['called'] for v in r['variants'])}/77")
print(f"Panel coverage: {r['qc']['panel_coverage']:.1f}%")
PY
```

This independently measured run's JSON contains `12.092248679` scan seconds. A rerun can vary; use its displayed value rather than claiming an exact repeat. This command writes a separate CLI report and is **not** used as a fake report for the subsequent browser scan.

### 3. Start the demo server

```sh
target/release/helixscan serve \
  --port 8780 --bind 127.0.0.1 \
  --input-dir bench/data --threads 4 \
  --store-dir helixscan-runs/demo
```

Open `http://127.0.0.1:8780/` in a desktop browser with WebGL enabled. Choose **`demo-1gb.fastq`**, set **Cores** to **4**, and leave **CPU · runtime SIMD** selected. Start recording only after the file size is displayed and the helix is visible. If the browser cannot create WebGL, fix that before recording; do not substitute a static screenshot. Use a normal browser rather than reduced-motion mode to show continuous helix rotation.

## Shot list (60 seconds)

### 0–10 s — Scope and setup

- Wide view of the scan page. Hold on the yellow **Research and education only. Not a medical diagnosis** banner for two seconds.
- Show **1.19 GB FASTQ** beside the selected `demo-1gb.fastq` (the UI's GB label uses the binary-byte conversion), and the **77 variants** marker panel.
- Say: “HelixScan analyzes a simulated FASTQ locally against a research marker panel. No human genome and no upload to an outside service.”
- Point briefly to the cyan/magenta helix and **CPU · runtime SIMD**, then click **Start scan**. Keep the whole scan layout in frame.

### 10–45 s — Real animation, live evidence and report opening

- Follow the helix as it rotates and its cyan scan ring rises with actual byte progress. Show **Live hits** changing and ref/alt vote bars accumulating; amber marks supporting evidence.
- Show the counters: throughput in GB/s, reads, bases, elapsed seconds, byte progress/ETA and per-core CPU bars. These are live SSE values, not a scripted timer or animation.
- Say: “The full four-million-read scan took about **12.1 seconds** on this four-worker run—not 45 seconds. I’m letting the completed report stay briefly on screen while explaining what was scanned.” The scan finishes well before the end of this segment. Do not claim it remains running until 45 seconds.
- **At completion (~12.1 s plus UI overhead), the page automatically navigates to that scan’s `/reports/<id>/report` page.** Do not click an old report or cut to a separate recording. If needed, give the browser a few seconds without talking while the report appears.
- Show the report filename and engine line, then the QC cards: 4,000,000 reads, 600,000,000 bases, Q30/GC and 100% panel coverage. Scroll to genotype/ref/alt-depth/GQ columns and depth per gene.
- Briefly show whole-sample checks and PGx associations. Explain that ancestry-like scoring is limited marker similarity, not ancestry or identity determination; a marker hit is not a disease prediction.

### 45–60 s — Save and limitations

- Click **Export JSON** once while keeping the report in view, then scroll to **What this cannot tell you**. Do not wait for a download dialog on camera.
- End on limitations: variants outside the panel, CYP2D6, full HLA type, age, blood pressure and disease certainty cannot be inferred. PGx findings require clinical confirmation and are never instructions to change medication.
- Closing line: “Local, reproducible research analysis from FASTQ to saved report—well inside the minute on this host.”

## Verification and reproducibility notes

- `web/app.js` opens the report URL returned with the newly created scan when its terminal SSE snapshot arrives. The report is written by the server after a real full scan; its measured scan seconds are inside that report.
- `web/helix.js` provides the live scan-fraction update and `setHit` method used by the scan UI. Browser console errors are a failed verification, not something to edit out of the recording.
- Re-run `cargo test --workspace` after source edits. `testdata/small.fastq` and tests establish correctness separately from this larger synthetic demo fixture. The optional saved-report browser regression needs local Playwright and Chromium:

```sh
PLAYWRIGHT_DIR=/path/to/playwright/package \
HELIXSCAN_CHROME=/path/to/chromium \
  cargo test -p helixscan-app --features browser-test --test saved_report_browser
```

- To record: use any 1080p desktop capture tool, keep this server bound to `127.0.0.1`, and include the actual click and automatic report navigation in one continuous take. Browser/capture tooling is not bundled into this repository.
