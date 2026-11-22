//! HG001/GIAB truth-check audit for a HelixScan JSON report.
//!
//! The shell driver only invokes containers. Every validation, comparison,
//! allele reconstruction and report-writing decision is implemented here.
use anyhow::{Context, Result, bail};
use helixscan_core::{Variant, report::Report};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const VCF_URL: &str = "https://ftp-trace.ncbi.nlm.nih.gov/ReferenceSamples/giab/release/NA12878_HG001/NISTv4.2.1/GRCh38/HG001_GRCh38_1_22_v4.2.1_benchmark.vcf.gz";
const BAM_URL: &str = "https://ftp-trace.ncbi.nlm.nih.gov/ReferenceSamples/giab/data/NA12878/NIST_NA12878_HG001_HiSeq_300x/NHGRI_Illumina300X_novoalign_bams/HG001.GRCh38_full_plus_hs38d1_analysis_set_minus_alts.300x.bam";
const SAMTOOLS_IMAGE: &str = "quay.io/biocontainers/samtools:1.22.1--h96c455f_0";
const BCFTOOLS_IMAGE: &str = "quay.io/biocontainers/bcftools:1.25--h118bc1c_1";

#[derive(Clone, Debug)]
struct Truth {
    coverage: String,
    genotype: Option<String>,
    vcf_gt: Option<String>,
    vcf: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct Evidence {
    alignments: usize,
    duplicate_alignments_removed: usize,
    spanning_reads: u32,
    eligible_reads: u32,
    allele_counts: BTreeMap<String, u32>,
    other_counts: BTreeMap<String, u32>,
    genotype: String,
    depth: u32,
    gq: u8,
}

#[derive(Serialize)]
struct SiteResult<'a> {
    #[serde(flatten)]
    variant: &'a Variant,
    truth: TruthResult,
    evidence: Evidence,
    scanner_rsid: String,
    scanner_called: bool,
    scanner_genotype: String,
    scanner_depth: u32,
    scanner_gq: u8,
    scanner_ref_votes: u32,
    scanner_alt_votes: Vec<u32>,
    scanner_no_call_reason: Option<String>,
    evaluable: bool,
    scanner_match: bool,
    read_match: bool,
}

#[derive(Serialize)]
struct TruthResult {
    coverage: String,
    genotype: Option<String>,
    vcf_gt: Option<String>,
    vcf: Option<String>,
}

fn raw_allele(a: &str) -> &str {
    if a == "-" { "" } else { a }
}

fn raw_ref(v: &Variant) -> String {
    if v.raw_ref.is_empty() {
        v.reference.clone()
    } else {
        raw_allele(&v.raw_ref).into()
    }
}

fn ref_span(v: &Variant) -> (u64, u64) {
    (
        v.pos,
        v.end.max(v.pos + raw_ref(v).len().saturating_sub(1) as u64),
    )
}

fn target(v: &Variant) -> u64 {
    v.pos + v.allele_offset as u64
}

fn read_region(v: &Variant) -> (String, u64, u64) {
    let chr = if v.chr == "MT" {
        "chrM".into()
    } else {
        format!("chr{}", v.chr)
    };
    // Two mapped flanks make adjacent alignment artifacts ineligible rather
    // than falsely accepting an incomplete allele.
    let (start, end) = ref_span(v);
    let start = start.saturating_sub(2).max(1);
    let end = end + 1;
    (format!("{chr}:{start}-{end}"), start, end.max(start))
}

fn panel_manifest(work: &Path, panel: &[Variant]) -> Result<()> {
    let mut tsv = "chr\tstart\tend\t0-based BED equivalent shown for provenance\n".to_string();
    let mut regions = String::new();
    let mut json = Vec::new();
    for v in panel {
        let (region, _, _) = read_region(v);
        regions += &format!("{}\t{}\n", v.rsid, region);
        json.push(serde_json::json!({
            "rsid": v.rsid, "chr": v.chr, "pos": v.pos,
            "allele_offset": v.allele_offset, "raw_ref": raw_ref(v),
            "raw_alts": v.raw_alts, "region": region,
            "truth_region": format!("chr{}:{}", v.chr, target(v)),
        }));
        tsv += &format!(
            "chr{}\t{}\t{}\t{}\n",
            v.chr,
            target(v) - 1,
            target(v),
            v.rsid
        );
    }
    fs::write(work.join("panel-sites.tsv"), tsv)?;
    fs::write(
        work.join("panel-sites.json"),
        serde_json::to_string_pretty(&json)? + "\n",
    )?;
    fs::write(
        work.join("regions.txt"),
        panel
            .iter()
            .map(|v| read_region(v).0 + "\n")
            .collect::<String>(),
    )?;
    fs::write(work.join("panel-regions.tsv"), regions)?;
    Ok(())
}

fn validate_site(v: &Variant, path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut output = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('@') {
            continue;
        }
        let f = line.split('\t').collect::<Vec<_>>();
        if f.len() < 11 {
            bail!("{}:{}: truncated SAM record", path.display(), n + 1);
        }
        let flags: u16 = f[1].parse()?;
        // 0x904 = unmapped | secondary | supplementary | duplicate | QC-fail.
        if flags & 0x904 != 0 {
            bail!(
                "{}:{}: non-primary alignment passed the checked filter",
                path.display(),
                n + 1
            );
        }
        let mapq: u8 = f[4].parse()?;
        if mapq < 20 {
            bail!("{}:{}: mapping quality is below 20", path.display(), n + 1);
        }
        let pos: u64 = f[3].parse()?;
        if f[5] == "*" {
            bail!("{}:{}: missing CIGAR", path.display(), n + 1);
        }
        let stop = reference_end(pos, f[5])?;
        let (region, start, end) = read_region(v);
        let expected_chr = region.split(':').next().unwrap();
        if f[2] != expected_chr || stop < start || pos > end {
            bail!("{}:{}: record lies outside {region}", path.display(), n + 1);
        }
        if f[9].len() != f[10].len() || f[9] == "*" {
            bail!("{}:{}: invalid SEQ/QUAL", path.display(), n + 1);
        }
        output.push(line.to_owned());
    }
    Ok(output)
}

fn reference_end(pos: u64, cigar: &str) -> Result<u64> {
    let mut end = pos;
    for (n, op) in cigar_ops(cigar)? {
        if matches!(op, 'M' | 'D' | 'N' | '=' | 'X') {
            end += n;
        }
    }
    Ok(end)
}

fn cigar_ops(cigar: &str) -> Result<Vec<(u64, char)>> {
    if cigar == "*" {
        bail!("unmapped CIGAR");
    }
    let mut n = 0u64;
    let mut result = Vec::new();
    for c in cigar.chars() {
        if c.is_ascii_digit() {
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(c as u64 - '0' as u64))
                .context("CIGAR overflow")?;
        } else {
            if n == 0 || !"MIDNSHP=X".contains(c) {
                bail!("invalid CIGAR {cigar}");
            }
            result.push((n, c));
            n = 0;
        }
    }
    if n != 0 || result.is_empty() {
        bail!("invalid CIGAR {cigar}");
    }
    Ok(result)
}

fn mapped_allele(v: &Variant, sam: &str) -> Result<Option<String>> {
    let f: Vec<_> = sam.split('\t').collect();
    let seq = f[9].as_bytes();
    let qual = f[10].as_bytes();
    let (_, original_end) = ref_span(v);
    let start = target(v);
    let alleles = std::iter::once(v.ref_allele.as_str())
        .chain(v.alts.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let indel = alleles.iter().any(|a| a.len() != alleles[0].len());
    if start == 0 || (v.ref_allele.is_empty() && original_end + 1 != start) {
        return Ok(None);
    }
    let mut rpos: u64 = f[3].parse()?;
    let mut qpos = 0usize;
    let mut allele = Vec::new();
    let mut qualities = Vec::new();
    let mut mapped = BTreeMap::new();
    for (n64, op) in cigar_ops(f[5])? {
        let n = n64 as usize;
        let consumes_query = matches!(op, 'M' | '=' | 'X' | 'I' | 'S');
        if qpos + usize::from(consumes_query) * n > seq.len() {
            bail!("CIGAR exceeds sequence");
        }
        match op {
            'M' | '=' | 'X' => {
                for i in 0..n {
                    mapped.insert(rpos + i as u64, qpos + i);
                }
                if indel {
                    for r in start.max(rpos)..=original_end.min(rpos + n64 - 1) {
                        let q = qpos + (r - rpos) as usize;
                        allele.push(seq[q]);
                        qualities.push(qual[q]);
                    }
                }
                qpos += n;
                rpos += n64;
            }
            'S' => qpos += n,
            'H' => {}
            'I' => {
                if indel && rpos >= start && rpos <= original_end + 1 {
                    allele.extend(&seq[qpos..qpos + n]);
                    qualities.extend(&qual[qpos..qpos + n]);
                }
                qpos += n;
            }
            'D' => {
                if indel && rpos <= original_end && rpos + n64 > start {
                    // A deletion allele must remove the whole normalized REF.
                    if rpos != start || n64 != v.ref_allele.len() as u64 || !allele.is_empty() {
                        return Ok(None);
                    }
                } else if rpos <= start && rpos + n64 > original_end {
                    return Ok(None);
                }
                rpos += n64;
            }
            'N' => rpos += n64,
            x => bail!("unsupported CIGAR operation {x}"),
        }
    }
    if qpos != seq.len() {
        bail!("CIGAR and SEQ lengths differ");
    }
    if !indel {
        let Some(&q) = mapped.get(&start) else {
            return Ok(None);
        };
        if qual[q] < 20 {
            return Ok(None);
        }
        return Ok(Some((seq[q] as char).to_string()));
    }
    let (Some(&left), Some(&right)) = (mapped.get(&(start - 1)), mapped.get(&(original_end + 1)))
    else {
        return Ok(None);
    };
    if qual[left] < 20 || qual[right] < 20 || qualities.iter().any(|q| *q < 20) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8(allele)?))
}

fn read_evidence(v: &Variant, lines: &[String]) -> Result<Evidence> {
    let mut evidence = Evidence::default();
    let mut seen = BTreeMap::new();
    let mut observed = BTreeMap::<String, u32>::new();
    let panel = std::iter::once(v.ref_allele.as_str())
        .chain(v.alts.iter().map(String::as_str))
        .collect::<Vec<_>>();

    for line in lines {
        let f = line.split('\t').collect::<Vec<_>>();
        let key = (f[0].to_string(), f[1]);
        if let Some(old) = seen.insert(key, line) {
            evidence.duplicate_alignments_removed += usize::from(old != line);
            continue;
        }
        evidence.alignments += 1;
        let pos: u64 = f[3].parse()?;
        if pos <= target(v) && reference_end(pos, f[5])? > target(v) {
            evidence.spanning_reads += 1;
        }
        if let Some(a) = mapped_allele(v, line)? {
            *observed.entry(a).or_default() += 1;
        }
    }
    for (a, n) in observed {
        if panel.contains(&a.as_str()) {
            *evidence.allele_counts.entry(a).or_default() += n;
        } else {
            *evidence.other_counts.entry(a).or_default() += n;
        }
    }
    evidence.eligible_reads =
        evidence.allele_counts.values().sum::<u32>() + evidence.other_counts.values().sum::<u32>();
    evidence.depth = evidence.allele_counts.values().sum();
    evidence.genotype = read_genotype(v, &evidence.allele_counts, evidence.depth);
    evidence.gq = read_gq(v, &evidence.allele_counts, evidence.depth);
    Ok(evidence)
}

fn read_genotype(v: &Variant, counts: &BTreeMap<String, u32>, depth: u32) -> String {
    let alleles: Vec<_> = std::iter::once(v.ref_allele.clone())
        .chain(v.alts.clone())
        .collect();
    if depth < 8 {
        return "no call".into();
    }
    if v.chr == "X" || v.chr == "Y" || v.chr == "MT" {
        return alleles
            .iter()
            .max_by_key(|a| counts.get(*a).copied().unwrap_or(0))
            .unwrap()
            .clone();
    }
    let best_alt = (0..v.alts.len())
        .max_by_key(|i| counts.get(&v.alts[*i]).copied().unwrap_or(0))
        .unwrap();
    let ref_n = counts.get(&v.ref_allele).copied().unwrap_or(0);
    let alt_n = counts.get(&v.alts[best_alt]).copied().unwrap_or(0);
    let e = 0.01f64;
    let logs = [
        ref_n as f64 * (1.0 - e).ln() + alt_n as f64 * e.ln(),
        -(depth as f64) * 2f64.ln(),
        ref_n as f64 * e.ln() + alt_n as f64 * (1.0 - e).ln(),
    ];
    let best = (0..3).max_by(|x, y| logs[*x].total_cmp(&logs[*y])).unwrap();
    match best {
        0 => format!("{0}/{0}", v.ref_allele),
        1 => format!("{}/{}", v.ref_allele, v.alts[best_alt]),
        _ => format!("{0}/{0}", v.alts[best_alt]),
    }
}

fn read_gq(v: &Variant, counts: &BTreeMap<String, u32>, depth: u32) -> u8 {
    if depth < 8 {
        return 0;
    }
    if v.chr == "X" || v.chr == "Y" {
        return 99;
    }
    let best_alt = (0..v.alts.len())
        .max_by_key(|i| counts.get(&v.alts[*i]).copied().unwrap_or(0))
        .unwrap();
    let r = counts.get(&v.ref_allele).copied().unwrap_or(0);
    let a = counts.get(&v.alts[best_alt]).copied().unwrap_or(0);
    let e = 0.01f64;
    let logs = [
        r as f64 * (1.0 - e).ln() + a as f64 * e.ln(),
        -(depth as f64) * 2f64.ln(),
        r as f64 * e.ln() + a as f64 * (1.0 - e).ln(),
    ];
    let max = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum = logs.iter().map(|x| (x - max).exp()).sum::<f64>();
    let max = logs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    (-10.0f64 * (1.0 - 1.0 / sum).max(1e-10).log10())
        .round()
        .clamp(0.0, 99.0) as u8
}

fn canonical(g: &str) -> BTreeMap<String, usize> {
    let mut alleles = BTreeMap::new();
    for a in g.split('/') {
        *alleles.entry(a.to_string()).or_default() += 1;
    }
    alleles
}

fn bed(v: &Variant, text: &str) -> Result<Option<bool>> {
    if !matches!(
        v.chr.as_str(),
        "1" | "2"
            | "3"
            | "4"
            | "5"
            | "6"
            | "7"
            | "8"
            | "9"
            | "10"
            | "11"
            | "12"
            | "13"
            | "14"
            | "15"
            | "16"
            | "17"
            | "18"
            | "19"
            | "20"
            | "21"
            | "22"
    ) {
        return Ok(None);
    }
    let chr = format!("chr{}", v.chr);
    let target = target(v);
    for (n, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f = line.split_whitespace().collect::<Vec<_>>();
        if f.len() < 3 {
            bail!("BED line {} is malformed", n + 1);
        }
        if f[0] != chr {
            continue;
        }
        let start: u64 = f[1].parse()?;
        let end: u64 = f[2].parse()?;
        if start < target && target <= end {
            return Ok(Some(true));
        }
    }
    Ok(Some(false))
}

fn trim_truth(pos: u64, reference: &str, alleles: &[String]) -> Result<(u64, Vec<String>)> {
    let mut prefix = 0usize;
    let all = std::iter::once(reference).chain(alleles.iter().map(String::as_str));
    let first_bases = reference.as_bytes();
    while all.clone().all(|a| a.len() > prefix)
        && all
            .clone()
            .all(|a| a.as_bytes()[prefix] == first_bases[prefix])
    {
        prefix += 1;
    }
    let alleles = std::iter::once(reference[prefix..].to_string())
        .chain(alleles.iter().map(|a| a[prefix..].to_string()));
    Ok((pos + prefix as u64, alleles.collect()))
}

fn truth(v: &Variant, text: &str, in_bed: Option<bool>) -> Result<Truth> {
    if in_bed.is_none() {
        return Ok(Truth {
            coverage: "outside_autosomal_benchmark".into(),
            genotype: None,
            vcf_gt: None,
            vcf: None,
        });
    }
    let high_confidence = in_bed.unwrap();
    let region = format!("chr{}", v.chr);
    let mut matches = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<_> = line.split('\t').collect();
        if f.len() < 10 {
            bail!("{} truth line {} is malformed", v.rsid, n + 1);
        }
        let pos: u64 = f[1].parse()?;
        if f[0] != region
            || pos > target(v)
            || pos + (f[3].len().saturating_sub(1) as u64) < target(v)
        {
            bail!("{} truth record outside requested coordinate", v.rsid);
        }
        if f[6] != "PASS" {
            continue;
        }
        let format: Vec<_> = f[8].split(':').collect();
        let gt_index = format
            .iter()
            .position(|f| *f == "GT")
            .context("truth lacks GT")?;
        let gt = f[9]
            .split(':')
            .nth(gt_index)
            .context("truth lacks sample GT")?
            .replace('|', "/");
        if gt.contains('.') {
            bail!("{} has missing GIAB GT", v.rsid);
        }
        let indices = gt
            .split('/')
            .map(|i| i.parse::<usize>())
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let vcf_alleles = std::iter::once(f[3].to_string())
            .chain(f[4].split(',').map(str::to_string))
            .collect::<Vec<_>>();
        let choices = indices
            .iter()
            .map(|i| {
                vcf_alleles
                    .get(*i)
                    .context("GT index beyond ALT list")
                    .cloned()
            })
            .collect::<Result<Vec<_>>>()?;
        let (normalized_pos, normalized) = trim_truth(pos, f[3], &choices)?;
        if normalized_pos != target(v) || normalized.first() != Some(&v.ref_allele) {
            continue; // A left-anchored neighboring event is not this allele.
        }
        let normalized = &normalized[1..];
        let panel_ref = std::iter::once(v.ref_allele.as_str())
            .chain(v.alts.iter().map(String::as_str))
            .collect::<Vec<_>>();
        let mut observed = Vec::new();
        for (i, allele) in normalized.iter().enumerate() {
            if allele == &v.ref_allele || v.alts.contains(allele) {
                observed.push(allele.clone());
            } else {
                bail!(
                    "{} truth allele {:?} is not a panel allele {:?}",
                    v.rsid,
                    allele,
                    panel_ref
                );
            }
            if i > 10 {
                bail!("unexpected ploidy");
            }
        }
        observed.sort();
        matches.push((observed.join("/"), gt, line.to_string()));
    }
    if matches.len() > 1 {
        bail!("{} multiple GIAB records at target", v.rsid);
    }
    if let Some((g, gt, vcf)) = matches.pop() {
        if !high_confidence {
            bail!("{} PASS record outside high-confidence BED", v.rsid);
        }
        return Ok(Truth {
            coverage: "PASS_truth_record".into(),
            genotype: Some(g),
            vcf_gt: Some(gt),
            vcf: Some(vcf),
        });
    }
    if high_confidence {
        Ok(Truth {
            coverage: "reference_in_benchmark_BED".into(),
            genotype: Some(format!("{0}/{0}", v.ref_allele)),
            vcf_gt: Some("0/0 (no PASS record in callable BED)".into()),
            vcf: None,
        })
    } else {
        Ok(Truth {
            coverage: "outside_high_confidence_BED".into(),
            genotype: None,
            vcf_gt: None,
            vcf: None,
        })
    }
}

fn counts_text(counts: &BTreeMap<String, u32>) -> String {
    if counts.is_empty() {
        return "none".into();
    }
    counts
        .iter()
        .map(|(a, n)| format!("{}:{n}", if a.is_empty() { "∅" } else { a }))
        .collect::<Vec<_>>()
        .join("; ")
}

fn scanner_votes(row: &SiteResult) -> String {
    std::iter::once(row.scanner_ref_votes)
        .chain(row.scanner_alt_votes.iter().copied())
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn report(work: &Path, rows: &[SiteResult], total_reads: u64) -> Result<()> {
    let evaluable = rows.iter().filter(|r| r.evaluable).count();
    let matches = rows.iter().filter(|r| r.scanner_match).count();
    let raw_matches = rows.iter().filter(|r| r.read_match).count();
    let mismatches = rows
        .iter()
        .filter(|r| r.evaluable && !r.scanner_match)
        .count();
    let truth_records = rows
        .iter()
        .filter(|r| r.truth.coverage == "PASS_truth_record")
        .count();
    let reference_sites = rows
        .iter()
        .filter(|r| r.truth.coverage == "reference_in_benchmark_BED")
        .count();
    let pct = 100.0 * matches as f64 / evaluable as f64;
    let raw_pct = 100.0 * raw_matches as f64 / evaluable as f64;
    let lines = &mut Vec::new();
    macro_rules! out { ($($arg:tt)*) => { lines.push(format!($($arg)*)); }; }
    out!("# H0b: public HG001 Genome in a Bottle truth check\n");
    out!("## Result\n");
    out!(
        "**HelixScan: {matches}/{evaluable} evaluable, confident autosomal genotypes concordant ({pct:.2}%).** {mismatches} genotype(s) are mismatches or no-calls. The independent CIGAR/base allele audit is concordant at {raw_matches}/{evaluable} ({raw_pct:.2}%).\n"
    );
    out!(
        "All {n} sites were examined. chrX rs1050828 and chrMT rs267606617 are outside the GIAB GRCh38 v4.2.1 1–22 benchmark and are **reported as outside, never counted**. Of the {evaluable} evaluable sites, {truth_records} have a PASS record and {reference_sites} have no PASS record within the companion high-confidence BED; only the latter infer homozygous reference. Failed queries cannot become reference calls.\n",
        n = rows.len()
    );
    out!(
        "The bounded FASTQ contains {total_reads} primary, non-duplicate, MQ≥20 alignments. This is public HG001/NA12878 data, not a user's genome. It is a 77-site research check, not clinical validation.\n"
    );
    out!("## Provenance\n");
    out!(
        "- Truth: `{VCF_URL}` (GIAB v4.2.1 GRCh38, autosomes; 125,932,193-byte complete remote resource; only coordinates retrieved).\n"
    );
    out!(
        "- BED: the adjacent `HG001_GRCh38_1_22_v4.2.1_benchmark.bed` (15,479,939 bytes; SHA-256 `dc3485e60447a3e863c34dfc063c3710ea5ed5efae50855c5197ba62538263b9`).\n"
    );
    out!(
        "- Reads: `{BAM_URL}` (Illumina 300x GRCh38 analysis set; complete remote resource 589,702,020,566 bytes, never downloaded whole).\n"
    );
    out!(
        "- Tools: `{SAMTOOLS_IMAGE}` (samtools 1.22.1) and `{BCFTOOLS_IMAGE}` (bcftools 1.25) in podman. Only public paths are used.\n"
    );
    out!("## Methods\n");
    out!(
        "1. The shell driver reads `regions.txt`, generated by the Rust tool from `data/panel.json`. Each samtools query has an explicit `chr:start-end`, including one left-anchor base. It excludes UNMAP, SECONDARY, QCFAIL, DUP and SUPPLEMENTARY, retains MQ≥20 alignments, and writes SAM plus FASTQ through samtools in podman. The full FASTQ is scanned with `target/release/helixscan scan --backend cpu`.\n"
    );
    out!(
        "2. The Rust checker validates every saved SAM record against its bounds and flags. Exact query-name/flag duplicate records are collapsed for the independent audit. SNVs use the mapped Q≥20 base. Indels reconstruct raw REF/ALT through CIGAR and require two Q≥20 mapped flanking anchors. Unsupported alleles remain `other`, never REF. Genotype multisets compare without allele-order sensitivity.\n"
    );
    out!(
        "3. One indexed `bcftools view -H URL chr:position` query per locus provides PASS GT. A successful empty response within the BED infers 0/0. No full VCF is retrieved. Scanner calls are read from the release CLI JSON report; raw counts are independent CIGAR evidence, not k-mer votes.\n"
    );
    out!("## Concordance and read counts\n");
    out!(
        "Scanner REF/ALT votes follow panel allele order. `other` is shown independently of supported REF/ALT alleles; ∅ denotes an empty allele. Low-depth no-calls are mismatches, not concordance.\n"
    );
    out!(
        "| rsID | Locus | GIAB | Independent read calls and REF/ALT counts | Scanner | Scanner REF/ALT votes (REF / ALT1 / ALT2 / ALT3) | Result |"
    );
    out!("|---|---|---|---|---|---|---|");
    for r in rows {
        let status = if !r.evaluable {
            "outside benchmark"
        } else if r.scanner_match {
            "concordant"
        } else if !r.scanner_called {
            "no call / mismatch"
        } else {
            "mismatch"
        };
        let truth = r.truth.genotype.as_deref().unwrap_or("not available");
        let raw = format!(
            "{} (DP {}, GQ {})",
            r.evidence.genotype, r.evidence.depth, r.evidence.gq
        );
        let scanner = format!(
            "{} (DP {}, GQ {})",
            r.scanner_genotype, r.scanner_depth, r.scanner_gq
        );
        out!(
            "| {} | chr{}:{}+{} | {} | {} ({}; other: {}) | {} | {} | {} |",
            r.variant.rsid,
            r.variant.chr,
            r.variant.pos,
            r.variant.allele_offset,
            truth,
            raw,
            counts_text(&r.evidence.allele_counts),
            counts_text(&r.evidence.other_counts),
            scanner,
            scanner_votes(r),
            status
        );
    }
    out!("\n## Mismatches / no-calls\n");
    if mismatches == 0 {
        out!("None. Every evaluable confident genotype agrees with GIAB.\n");
    }
    for r in rows.iter().filter(|r| r.evaluable && !r.scanner_match) {
        out!(
            "### {} ({}, chr{}:{})\n",
            r.variant.rsid,
            r.variant.gene,
            r.variant.chr,
            r.variant.pos
        );
        out!(
            "GIAB: **{}**. HelixScan: **{}** (DP {}, GQ {}; REF/ALT votes {}). Independent read evidence: **{}** (DP {}, GQ {}), REF/ALT counts **{}**, other **{}**, {} spanning alignments, {} allele-eligible reads.\n",
            r.truth.genotype.as_deref().unwrap_or("not available"),
            r.scanner_genotype,
            r.scanner_depth,
            r.scanner_gq,
            scanner_votes(r),
            r.evidence.genotype,
            r.evidence.depth,
            r.evidence.gq,
            counts_text(&r.evidence.allele_counts),
            counts_text(&r.evidence.other_counts),
            r.evidence.spanning_reads,
            r.evidence.eligible_reads
        );
        out!(
            "The independent counts show whether the disagreement comes from a caller rule or from reads failing to produce allele k-mer votes. This site is not silently counted as homozygous reference.\n"
        );
    }
    out!("## Sites outside this truth dataset\n");
    for r in rows.iter().filter(|r| !r.evaluable) {
        out!(
            "- **{} ({})**: {}; scanner {} (DP {}), independent REF/ALT counts {}, other {}. Excluded from concordance.\n",
            r.variant.rsid,
            r.variant.gene,
            r.truth.coverage,
            r.scanner_genotype,
            r.scanner_depth,
            counts_text(&r.evidence.allele_counts),
            counts_text(&r.evidence.other_counts)
        );
    }
    out!("## Limitations\n");
    out!(
        "HG001 contains no panel-indel carrier genotype in the evaluable truth set. Independent indel logic has unit tests; this check does not establish accuracy in an unseen indel carrier or certify phased APOE haplotypes. A bounded locus FASTQ is not a whole-genome sample, so sample-level sex/QC heuristics are not validated by this report.\n"
    );
    out!("## Reproduce\n");
    out!("From the repository root, with podman and network access:\n");
    out!("```sh\nscripts/rerun_truth_check.sh\n```\n");
    out!(
        "The script builds the release scanner and Rust checker, performs 77 bounded public BAM queries and 77 bounded GIAB truth-coordinate queries, scans the resulting FASTQ, checks all 77 genotypes and regenerates this report plus `helixscan-runs/truth-check/results.json`. That directory is gitignored. No private or personal file is used.\n"
    );
    fs::write(work.join("truth-check.md"), lines.join("\n"))?;
    Ok(())
}

fn run(args: &[String]) -> Result<PathBuf> {
    if args.len() < 3 || args.len() > 5 || (args.len() > 3 && args.len() < 4) {
        bail!("usage: truth-check-genotypes PANEL WORK REPORT.md [SCAN.json --fetch-truth]");
    }
    let fetch = args.len() == 5 && args[4] == "--fetch-truth";
    if args.len() == 5 && !fetch {
        bail!("unknown option {}", args[4]);
    }
    let panel: helixscan_core::Panel = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    if panel.variants.len() != 77 {
        bail!("panel must contain exactly 77 variants");
    }
    let work = PathBuf::from(&args[1]);
    fs::create_dir_all(&work)?;
    panel_manifest(&work, &panel.variants)?;
    if args.len() == 3 {
        return Ok(work);
    }
    if fetch {
        fetch_truth(&work, &panel.variants)?;
    }
    let scan: Report = serde_json::from_str(&fs::read_to_string(&args[3])?)?;
    if scan.qc.reads == 0 {
        bail!("scanner report contains zero reads");
    }
    let bed_text = fs::read_to_string(work.join("HG001_GRCh38_v4.2.1_benchmark.bed"))?;
    let mut rows = Vec::new();
    for v in &panel.variants {
        let sam_path = work.join("alignments").join(format!("{}.sam", v.rsid));
        let lines = validate_site(v, &sam_path)?;
        let evidence = read_evidence(v, &lines)?;
        let truth_path = work.join("truth-sites").join(format!("{}.vcf", v.rsid));
        let truth_vcf =
            fs::read_to_string(&truth_path).with_context(|| format!("read {truth_path:?}"))?;
        let truth = truth(v, &truth_vcf, bed(v, &bed_text)?)?;
        let call = scan
            .variants
            .iter()
            .find(|c| c.rsid == v.rsid)
            .context("scanner report missing rsid")?;
        let evaluable = truth.genotype.is_some();
        let scanner_match = evaluable
            && call.called
            && canonical(&call.genotype) == canonical(truth.genotype.as_ref().unwrap());
        let read_match = evaluable
            && canonical(&evidence.genotype) == canonical(truth.genotype.as_ref().unwrap());
        rows.push(SiteResult {
            variant: v,
            truth: TruthResult {
                coverage: truth.coverage,
                genotype: truth.genotype,
                vcf_gt: truth.vcf_gt,
                vcf: truth.vcf,
            },
            evidence,
            scanner_rsid: call.rsid.clone(),
            scanner_called: call.called,
            scanner_genotype: call.genotype.clone(),
            scanner_depth: call.depth,
            scanner_gq: call.gq,
            scanner_ref_votes: call.ref_votes,
            scanner_alt_votes: call.alt_votes.clone(),
            scanner_no_call_reason: call.no_call_reason.clone(),
            evaluable,
            scanner_match,
            read_match,
        });
    }
    fs::write(
        work.join("results.json"),
        serde_json::to_string_pretty(&serde_json::json!({"rows":rows}))? + "\n",
    )?;
    report(&work, &rows, scan.qc.reads)?;
    fs::copy(work.join("truth-check.md"), &args[2])?;
    let good = rows.iter().filter(|r| r.scanner_match).count();
    let denominator = rows.iter().filter(|r| r.evaluable).count();
    println!(
        "HelixScan {good}/{denominator} ({:.2}%); all 77 sites reported",
        100.0 * good as f64 / denominator as f64
    );
    Ok(work)
}

fn fetch_truth(work: &Path, panel: &[Variant]) -> Result<()> {
    let out = work.join("truth-sites");
    fs::create_dir_all(&out)?;
    for v in panel {
        let region = format!("chr{}:{}", v.chr, target(v));
        let output = std::process::Command::new("podman")
            .args([
                "run",
                "--rm",
                "--network",
                "host",
                BCFTOOLS_IMAGE,
                "bcftools",
                "view",
                "-H",
                VCF_URL,
                &region,
            ])
            .output()?;
        if !output.status.success() {
            bail!(
                "{} truth query failed: {}",
                v.rsid,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let text = String::from_utf8(output.stdout)?;
        // Validate every queried response before retaining even an empty one.
        for line in text.lines() {
            let f = line.split('\t').collect::<Vec<_>>();
            let pos: u64 = f[1].parse()?;
            if f.len() < 10
                || f[0] != format!("chr{}", v.chr)
                || pos > target(v)
                || pos + (f[3].len().saturating_sub(1) as u64) < target(v)
            {
                bail!("truth response does not overlap {region}");
            }
        }
        fs::write(out.join(format!("{}.vcf", v.rsid)), text)?;
        println!(
            "truth {} {}: {} record(s)",
            v.rsid,
            region,
            fs::read_to_string(out.join(format!("{}.vcf", v.rsid)))?
                .lines()
                .count()
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    run(&std::env::args().skip(1).collect::<Vec<_>>()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v() -> Variant {
        serde_json::from_value(serde_json::json!({
            "rsid":"test","gene":"test","trait_group":"test","section":"health","note":"",
            "chr":"1","pos":100,"end":100,"reference":"A","raw_ref":"A","raw_alts":["G"],
            "ref":"A","alts":["G"],"flank_left":"","flank_right":"",
            "phenotypes":[],"clinical_significance":[],"frequencies":{},"fetched_at":"test","ensembl_release":116
        })).unwrap()
    }
    fn line(cigar: &str, seq: &str, pos: u64, v: &Variant) -> String {
        format!(
            "read\t0\tchr{}\t{pos}\t60\t{cigar}\t*\t0\t0\t{seq}\t{}",
            v.chr,
            "I".repeat(seq.len())
        )
    }
    #[test]
    fn snv_with_clipping_and_low_quality() {
        let v = v();
        assert_eq!(
            mapped_allele(&v, &line("3M", "CAG", 99, &v))
                .unwrap()
                .as_deref(),
            Some("A")
        );
        assert_eq!(
            mapped_allele(&v, &line("1S2M", "TCG", 99, &v))
                .unwrap()
                .as_deref(),
            Some("G")
        );
        let low = line("3M", "CAG", 99, &v).replacen('I', "\x05", 2);
        assert!(mapped_allele(&v, &low).unwrap().is_none());
        assert!(
            mapped_allele(&v, &line("1M1D2M", "CAA", 99, &v))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            mapped_allele(&v, &line("2M1D1M", "CAA", 99, &v))
                .unwrap()
                .as_deref(),
            Some("A")
        );
    }
    #[test]
    fn reconstructs_deletion_and_rejects_missing_anchor() {
        let mut v = v();
        v.pos = 100;
        v.end = 102;
        v.allele_offset = 1;
        v.raw_ref = "CTA".into();
        v.raw_alts = vec!["C".into()];
        v.reference = "CTA".into();
        v.ref_allele = "TA".into();
        v.alts = vec!["".into()];
        assert_eq!(
            mapped_allele(&v, &line("2M2D3M", "CCGGT", 99, &v))
                .unwrap()
                .as_deref(),
            Some("")
        );
        assert!(
            mapped_allele(&v, &line("2M2D3M", "CCGGT", 100, &v))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            mapped_allele(&v, &line("7M", "CCTAGGT", 99, &v))
                .unwrap()
                .as_deref(),
            Some("TA")
        );
        assert!(
            mapped_allele(&v, &line("1M4D3M", "CGGT", 99, &v))
                .unwrap()
                .is_none()
        );
        assert!(
            mapped_allele(&v, &line("1M3D2M", "CGG", 99, &v))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn reconstructs_insertion_and_empty_ref() {
        let mut v = v();
        v.pos = 100;
        v.end = 102;
        v.allele_offset = 3;
        v.raw_ref = "GGG".into();
        v.raw_alts = vec!["GGGG".into()];
        v.reference = "GGG".into();
        v.ref_allele = "".into();
        v.alts = vec!["G".into()];
        assert_eq!(
            mapped_allele(&v, &line("3M1I3M", "TGGGTAA", 100, &v))
                .unwrap()
                .as_deref(),
            Some("G")
        );
        assert_eq!(
            mapped_allele(&v, &line("6M", "TGGGAA", 100, &v))
                .unwrap()
                .as_deref(),
            Some("")
        );
        // No right anchor: do not call an incomplete insertion allele.
        let mut short = v.clone();
        short.pos = 100;
        short.end = 105;
        short.raw_ref = "GGG".into();
        assert!(
            mapped_allele(&short, &line("3M1I2M", "TGGGTA", 100, &short))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn truth_mapping_and_bed_semantics() {
        let mut indel = v();
        indel.pos = 100;
        indel.allele_offset = 1;
        indel.raw_ref = "AT".into();
        indel.raw_alts = vec!["GT".into()];
        indel.raw_alts = vec!["GT".into()];
        indel.ref_allele = "T".into();
        indel.alts = vec!["G".into()];
        let row = "chr1\t100\t.\tAT\tAG\t50\tPASS\t.\tGT\t1/1";
        assert_eq!(
            bed(&indel, "chr1\t99\t100\nchr1\t100\t101\n").unwrap(),
            Some(true)
        );
        assert_eq!(bed(&indel, "chr1\t100\t100\n").unwrap(), Some(false));
        let mut x = indel.clone();
        x.chr = "X".into();
        assert_eq!(bed(&x, "").unwrap(), None);
        let t = truth(&indel, &format!("{row}\n"), Some(true)).unwrap();
        // Target is 101; normalization makes GT the observed allele.
        assert_eq!(t.genotype.as_deref(), Some("G/G"));
        let v = v();
        let row = "chr1\t100\t.\tA\tG\t50\tPASS\t.\tGT:DP\t0/1:20";
        assert_eq!(
            truth(&v, &format!("{row}\n"), Some(true))
                .unwrap()
                .genotype
                .as_deref(),
            Some("A/G")
        );
        assert_eq!(
            truth(&v, "", Some(false)).unwrap().coverage,
            "outside_high_confidence_BED"
        );
        assert!(canonical("C/T") == canonical("T/C"));
    }
    #[test]
    fn no_pass_record_is_reference_only_in_bed() {
        let v = v();
        assert_eq!(
            truth(&v, "", Some(true)).unwrap().genotype.as_deref(),
            Some("A/A")
        );
        assert_eq!(truth(&v, "", Some(false)).unwrap().genotype, None);
    }
    #[test]
    fn rejects_non_primary_records() {
        let v = v();
        let record = line("3M", "CAG", 99, &v).replace("read\t0\t", "read\t256\t");
        let path = std::env::temp_dir().join("helixscan-invalid-test.sam");
        fs::write(&path, record + "\n").unwrap();
        assert!(validate_site(&v, &path).is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn accepts_sam_headers() {
        let path = std::env::temp_dir().join("helixscan-headed-test.sam");
        fs::write(
            &path,
            format!("@HD\tVN:1.0\n{}\n", line("3M", "CAG", 99, &v())),
        )
        .unwrap();
        assert_eq!(validate_site(&v(), &path).unwrap().len(), 1);
        fs::remove_file(path).unwrap();
    }
}
