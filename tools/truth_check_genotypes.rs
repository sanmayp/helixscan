//! Use HelixScan's production allele voting on bounded HG001 BAM reads.
//! The main truth-check binary performs an independent SAM/CIGAR audit.
use anyhow::Result;
use helixscan_core::{Panel, PanelIndex, genotype::call, index::{ReadHit, scan_read_with}};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};
#[derive(Deserialize)]
struct Site { rsid: String, region: String, chr: String, sam: String }
#[derive(Default, serde::Serialize)]
struct SiteResult {
    rsid: String, region: String, chr: String,
    alignments: usize, duplicate_alignments_removed: usize,
    scanned_reads: usize, short_reads: usize, votes: Vec<u32>,
    called: bool, genotype: String, depth: u32, gq: u8,
    no_call_reason: Option<String>,
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let panel: Panel = serde_json::from_str(&fs::read_to_string(args.next().unwrap())?)?;
    let input = PathBuf::from(args.next().unwrap());
    let output = PathBuf::from(args.next().unwrap());
    assert!(args.next().is_none());
    let index = PanelIndex::new(&panel);
    let mut results = Vec::new();
    for entry in fs::read_dir(input)? {
        let site: Site = serde_json::from_str(&fs::read_to_string(entry?.path())?)?;
        let id = panel.variants.iter().position(|v| v.rsid == site.rsid)
            .ok_or_else(|| anyhow::anyhow!("unknown rsid {}", site.rsid))?;
        let mut counts = vec![[0u32; 4]; panel.variants.len()];
        let mut encoder = helixscan_core::encode::KmerEncoder::new(index.simd);
        let mut hits: Vec<ReadHit> = Vec::new();
        let mut seen = BTreeMap::new();
        let mut alignments = 0;
        let mut duplicate_alignments_removed = 0;
        let mut scanned_reads = 0;
        let mut short_reads = 0;
        for line in site.sam.lines() {
            alignments += 1;
            let f = line.split('\t').collect::<Vec<_>>();
            if f.len() < 11 { anyhow::bail!("invalid SAM record for {}", site.rsid); }
            let key = (f[0].to_owned(), f[1].parse::<u16>()?);
            if let Some(previous) = seen.get(&key) {
                if previous != &line { duplicate_alignments_removed += 1; }
                continue;
            }
            seen.insert(key, line.to_owned());
            let seq = f[9].as_bytes();
            let qual = f[10].as_bytes();
            if seq.len() != qual.len() { anyhow::bail!("SAM SEQ/QUAL lengths differ"); }
            if seq.len() < helixscan_core::KMER_SIZE { short_reads += 1; continue; }
            scanned_reads += 1;
            scan_read_with(&index, seq, qual, &mut counts, &mut encoder, &mut hits);
        }
        let c = call(&panel.variants[id], &counts[id], "XY");
        results.push(SiteResult {
            rsid: site.rsid, region: site.region, chr: site.chr,
            alignments, duplicate_alignments_removed, scanned_reads, short_reads,
            votes: counts[id].into_iter().take(panel.variants[id].alts.len() + 1).collect(),
            called: c.called, genotype: c.genotype, depth: c.depth, gq: c.gq,
            no_call_reason: c.no_call_reason,
        });
    }
    results.sort_by(|a, b| a.rsid.cmp(&b.rsid));
    fs::write(output, serde_json::to_string_pretty(&results)? + "\n")?;
    Ok(())
}
