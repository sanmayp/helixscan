#!/usr/bin/env bash
# Bounded public HG001 truth check. No Python and no private data files.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
WORK=helixscan-runs/truth-check
VCF='https://ftp-trace.ncbi.nlm.nih.gov/ReferenceSamples/giab/release/NA12878_HG001/NISTv4.2.1/GRCh38/HG001_GRCh38_1_22_v4.2.1_benchmark.vcf.gz'
BED=${VCF%.vcf.gz}.bed
BAM='https://ftp-trace.ncbi.nlm.nih.gov/ReferenceSamples/giab/data/NA12878/NIST_NA12878_HG001_HiSeq_300x/NHGRI_Illumina300X_novoalign_bams/HG001.GRCh38_full_plus_hs38d1_analysis_set_minus_alts.300x.bam'
SAMTOOLS='quay.io/biocontainers/samtools:1.22.1--h96c455f_0'
BCFTOOLS='quay.io/biocontainers/bcftools:1.25--h118bc1c_1'

mkdir -p "$WORK/alignments" "$WORK/reads" "$WORK/truth-sites"
cargo build --release --bin helixscan --bin truth-check-genotypes

# A tiny shell-only manifest generator is not needed: all panel logic is Rust.
target/release/truth-check-genotypes data/panel.json "$WORK" docs/TRUTH-CHECK.md

if [ ! -s "$WORK/HG001_GRCh38_v4.2.1_benchmark.bed" ]; then
    curl --fail --location --retry 8 --retry-all-errors --retry-delay 5 \
        --output "$WORK/HG001_GRCh38_v4.2.1_benchmark.bed.part" "$BED"
    mv "$WORK/HG001_GRCh38_v4.2.1_benchmark.bed.part" "$WORK/HG001_GRCh38_v4.2.1_benchmark.bed"
fi

# Mount only this ignored public-data workspace, not a personal directory.
samtools() {
    podman run --rm --network host -v "$ROOT/$WORK":/data:rw,z "$SAMTOOLS" samtools "$@"
}
bcftools() {
    podman run --rm --network host -v "$ROOT/$WORK":/data:rw,z "$BCFTOOLS" bcftools "$@"
}

: > "$WORK/reads.fastq"
while IFS= read -r line; do
    rsid=${line%%$'\t'*}
    region=${line#*$'\t'}
    if [ ! -s "$WORK/alignments/$rsid.sam" ] || [ ! -s "$WORK/reads/$rsid.fastq" ]; then
        samtools view --min-MQ 20 \
            --exclude-flags UNMAP,SECONDARY,QCFAIL,DUP,SUPPLEMENTARY \
            -O BAM -o "/data/alignments/$rsid.bam" "$BAM" "$region"
        samtools view -h "/data/alignments/$rsid.bam" -o "/data/alignments/$rsid.sam"
        samtools fastq -N "/data/alignments/$rsid.bam" -o "/data/reads/$rsid.fastq"
        printf '%s %s\n' "$rsid" "$region"
    fi
    cat "$WORK/reads/$rsid.fastq" >> "$WORK/reads.fastq"
done < "$WORK/panel-regions.tsv"

target/release/helixscan scan "$WORK/reads.fastq" --panel data/panel.json \
    --backend cpu --threads 4 --out "$WORK/scan-report.json"

# Truth requests are separate, checked, single-coordinate queries. Their output
# files are intentionally not derived from the scanner or read evidence.
target/release/truth-check-genotypes data/panel.json "$WORK" \
    docs/TRUTH-CHECK.md "$WORK/scan-report.json" --fetch-truth
target/release/truth-check-genotypes data/panel.json "$WORK" \
    docs/TRUTH-CHECK.md "$WORK/scan-report.json"

echo "Wrote docs/TRUTH-CHECK.md and $WORK/results.json"
