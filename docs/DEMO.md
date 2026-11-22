# 60-second research demo

This demo serves an approximately 20 GB synthetic FASTQ to the interactive helix and report UI. The panel and web dependencies are local; no network is needed once the project and fixture are built.

## Prepare before the minute starts

From the repository root, build the optimized binary:

```sh
cargo build --release
```

Generate this fixture ahead of time if it is not present. Generation is intentionally outside the 60 seconds.

```sh
mkdir -p bench/data
cargo run --release -p helixscan-sim -- \
  --panel data/panel.json --out bench/data/demo-20gb.fastq \
  --reads 63492064 --len 150 --depth 30 --seed 71 \
  --truth bench/data/demo-20gb.truth.json
```

The file in this repository's measured run is 20,227,006,292 bytes and 63,492,064 reads. It is synthetic data for performance and correctness testing, not a human genome. The recorded four-worker CPU AVX-512 scan took **326.662166 seconds** and reported **100% (77/77) simulated genotype agreement**. A guided 60-second presentation therefore previews the scan and opens a previously completed saved report; it does not promise the 20 GB scan itself completes in one minute. A warm page cache is realistic for an immediate repeat demonstration; do not describe a first cached run as a cold-read result.

For the saved-report segment, first create it with the optional CLI command below. Then add the missing web wrapper using the fixture's actual byte size (the wrapper does not rerun or alter report values):

```sh
python3 - <<'PY'
import hashlib, json, pathlib
source = pathlib.Path('bench/data/demo-20gb.fastq')
report = json.load(open('helixscan-runs/demo-20gb-report.json'))
id = hashlib.sha256(str(source.stat().st_ino).encode()).hexdigest()[:32]
store = pathlib.Path('helixscan-runs/demo'); store.mkdir(exist_ok=True)
engine = {key: None for key in ('fallback_reason', 'gpu_device', 'gpu_driver',
                                'gpu_memory_mb', 'workgroup', 'memory_cap_mb')}
engine['description'] = report['engine']
saved = dict(report_id=id, report=report, engine=engine,
             file_size=source.stat().st_size, filename=source.name)
json.dump(saved, (store / f'{id}.json').open('w'), indent=2)
print(f'Open http://127.0.0.1:8780/reports/{id}/report at 35 seconds.')
print(f'JSON: http://127.0.0.1:8780/api/scans/{id}/report.json')
PY
```

Start the UI in one terminal:

```sh
target/release/helixscan serve \
  --port 8780 --bind 127.0.0.1 \
  --input-dir bench/data --threads 4 \
  --store-dir helixscan-runs/demo
```

Open `http://127.0.0.1:8780/` before beginning. Select `demo-20gb.fastq` and **CPU · runtime SIMD**. Its measured 60-second behavior depends on the host and whether the 20 GB file is cached; see `bench/RESULTS.md` for recorded timings. This is a guided one-minute presentation, not a universal promise that every host finishes in under a minute.

## The minute

**0–10 seconds — Explain the scope.** Show the yellow research banner: this is an association summary, not a medical diagnosis. The file size, format and 77-variant panel are visible before scanning. No human data leaves this local server.

**10–20 seconds — Start and watch the real progress.** Press **Start scan**. Point out cyan/magenta instanced double-helix strands and the glowing ring moving with the real byte fraction. The large throughput value, reads, bases, elapsed time and bottom progress/ETA come from SSE—not a prerecorded animation. CPU telemetry is read from `/proc/stat` by the local server.

**20–35 seconds — Inspect preview evidence.** Mention individual live gene tags and ref/alt votes; amber indicates supporting allele evidence. A tag is not a disease prediction. Explain that full panel evidence accumulates throughout the run: on this host the recorded 20 GB scan took 326.662 seconds, so the first minute cannot show a complete result for this fixture. Open the separately prepared saved report next, rather than suggesting the foreground scan has finished.

**35–50 seconds — Open the prepared report.** Open the URL printed by the wrapper script. This is the completed 20 GB scan, not the still-running foreground preview. Describe QC (reads, bases, Q30, GC, N and panel coverage), the genotype table, ref/alt depth and genotype-quality bars. Show depth per gene and the small population-similarity comparison. The latter uses only the available markers and is explicitly rough, not ancestry or identity determination.

**50–60 seconds — Save and stop at the limits.** Use **Export JSON** or **Print / PDF**. End on **What this cannot tell you**: rare variants outside the panel, CYP2D6, full HLA type, age, blood pressure and disease certainty. Pharmacogenetic summaries require clinical confirmation and are never instructions to change medication.

## Optional command-line equivalent

```sh
time target/release/helixscan scan bench/data/demo-20gb.fastq \
  --panel data/panel.json --backend cpu --threads 4 \
  --out helixscan-runs/demo-20gb-report.json
```

For automated interface checks, use `curl` against `/`, `/app.js`, `/helix.js`, `/styles.css`, `/responsive.css`, `/visuals.css`, `/vendor/three.module.js`, `/api/files`, `/api/panel` and `/api/telemetry`. Submit JSON to `/api/scans`, read the returned `id`, stream `/api/scans/<id>/events`, then retrieve `/api/scans/<id>/report.json` and `/reports/<id>/report`. A persisted report can also be reopened through `/reports/<id>` and `/reports/<id>/events` after a server restart; its `/api/scans/<id>/report.json` route serves the same saved JSON. M7 verified all four saved-report routes for the 20 GB artifact under ID `3a3db3a72e4cf63b71d19e6050e5554f`; the wrapper prints the ID for a regenerated fixture. The browser's saved report HTML shares the report UI, but an HTTP `curl` check does not constitute a visual phone-layout test.
