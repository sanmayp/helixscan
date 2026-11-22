#!/bin/sh
set -eu
# M7 measurements: no overlapping scanner/benchmark workloads.
# Existing baseline one-thread, 20-GB, and 10-GB GPU runs are preserved separately.
# Cache status is reported as first/repeat sequential access, not cold/warm proof.
export VK_ICD_FILENAMES=/home/genescan/.config/vulkan/nvidia_icd.json
mkdir -p helixscan-runs
: "${M7_PHASE:=scaling}"
if [ "$M7_PHASE" = scaling ]; then
  if grep -q '^phase=scaling-complete$' helixscan-runs/m7-scaling.stdout 2>/dev/null; then
    echo 'scaling already completed; use a distinct output log before a deliberate rerun' >&2
    exit 1
  fi
  for threads in 2 4 6 11; do
    for pass in first repeat; do
      echo "fixture=bench/data/10gb.fastq build=M7-optimized threads=$threads pass=$pass"
      target/release/helixscan-kmer-bench \
        --input bench/data/10gb.fastq --threads "$threads" --repeats 1 --encoding-mb 16
    done
  done
  echo "fixture=bench/data/50gb.fastq build=M7-optimized threads=4 pass=first"
  target/release/helixscan-kmer-bench \
    --input bench/data/50gb.fastq --threads 4 --repeats 1 --encoding-mb 16
  echo "phase=scaling-complete"
elif [ "$M7_PHASE" = gpu50 ]; then
  # The forced path reports its actual adapter and preserves CPU vote/QC semantics.
  if test -f helixscan-runs/50gb-gpu-report.json; then
    echo '50 GB GPU report already exists; inspect it before any deliberate rerun' >&2
    exit 1
  fi
  echo "fixture=bench/data/50gb.fastq build=M7-optimized threads=4 backend=gpu"
  target/release/helixscan scan bench/data/50gb.fastq \
    --panel data/panel.json --backend gpu --threads 4 --gpu-mem-mb 2048 \
    --out helixscan-runs/50gb-gpu-report.json
  echo "phase=gpu50-complete"
else
  echo "unknown M7_PHASE=$M7_PHASE" >&2
  exit 2
fi
