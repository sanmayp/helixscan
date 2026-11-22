use crate::encode::SimdLevel;
use crate::{Panel, genotype::GenotypeCall};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SampleChecks {
    pub sex: SexReport,
    pub telomere: TelomereReport,
    pub ancestry: Vec<AncestryScore>,
    pub ancestry_summary: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SexReport {
    pub classification: String,
    pub y_reads_per_million: f64,
    pub control_reads_per_million: f64,
    pub evidence: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TelomereReport {
    pub telomeric_reads: u64,
    pub reads_per_million: f64,
    pub length_adjusted_index: f64,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AncestryScore {
    pub population: String,
    pub log_likelihood: f64,
    pub difference: f64,
    pub markers: usize,
}
#[derive(Clone, Debug, Default)]
pub struct MarkerLocal {
    pub y: u64,
    pub control: u64,
    pub telomeric: u64,
}
impl MarkerLocal {
    pub fn merge(&mut self, other: &Self) {
        self.y += other.y;
        self.control += other.control;
        self.telomeric += other.telomeric;
    }
}
#[derive(Debug, Default)]
pub struct MarkerIndex {
    pub y: HashSet<u64>,
    pub control: HashSet<u64>,
    pub x: HashSet<u64>,
}
impl MarkerIndex {
    pub fn new(panel: &Panel) -> Self {
        let keys = |name: &str| {
            panel
                .marker_regions
                .iter()
                .filter(|r| r.name == name)
                .flat_map(|r| {
                    r.sequence
                        .as_bytes()
                        .windows(crate::KMER_SIZE)
                        .filter_map(crate::kmer)
                })
                .collect::<HashSet<_>>()
        };
        let mut x = keys("RPS4X");
        x.extend(keys("KDM6A"));
        let control = keys("ACTB");
        let mut y = keys("SRY");
        y.extend(keys("UTY"));
        y.retain(|key| !x.contains(key) && !control.contains(key));
        Self { y, control, x }
    }
}

/// Share immutable k-mer sets and reuse packed-base/key buffers per worker.
pub struct MarkerScanner {
    pub index: Arc<MarkerIndex>,
    encoder: crate::encode::KmerEncoder,
    keys: Vec<crate::encode::KmerKey>,
}
impl MarkerScanner {
    pub fn new(index: Arc<MarkerIndex>, level: SimdLevel) -> Self {
        Self {
            index,
            encoder: crate::encode::KmerEncoder::new(level),
            keys: Vec::new(),
        }
    }
    pub fn add(&mut self, seq: &[u8], local: &mut MarkerLocal) {
        if tandem(seq, b"TTAGGG") >= 7 || tandem(seq, b"CCCTAA") >= 7 {
            local.telomeric += 1;
        }
        let mut y = false;
        let mut control = false;
        self.encoder.kmer_keys_into(seq, &mut self.keys);
        for key in &self.keys {
            y |= self.index.y.contains(&key.canonical);
            if self.index.x.contains(&key.canonical) {
                // X-homolog matches are evidence against Y specificity, even if
                // a read happens to contain both marker and homolog sequence.
                y = false;
            }
            control |= self.index.control.contains(&key.canonical);
        }
        local.y += u64::from(y);
        local.control += u64::from(control);
    }
}
pub fn tandem(seq: &[u8], motif: &[u8]) -> usize {
    let mut best = 0;
    for start in 0..motif.len() {
        let mut n = 0;
        for b in seq.iter().skip(start) {
            if Some(b) == motif.get(n % motif.len()) {
                n += 1;
                best = best.max(n / motif.len());
            } else {
                n = 0;
            }
        }
    }
    best
}
pub fn sample_checks(
    panel: &Panel,
    calls: &[GenotypeCall],
    qc: &crate::qc::Qc,
    m: &MarkerLocal,
) -> SampleChecks {
    let reads = qc.reads as f64;
    if qc.reads == 0 {
        return SampleChecks::default();
    }
    let y = m.y as f64 / reads * 1e6;
    let control = m.control as f64 / reads * 1e6;
    // Deliberately broad research heuristics, not a sex or identity determination.
    let sex = if y > control && y > 1_000.0 {
        "XY"
    } else if y < control * 0.05 && control > 1_000.0 {
        "XX"
    } else {
        "unclear"
    };
    let ancestry = ancestry(panel, calls);
    let markers = ancestry.first().map_or(0, |x| x.markers);
    let ancestry_summary = if markers == 0 {
        "No population-frequency markers met the depth threshold.".into()
    } else if let Some(best) = ancestry.first() {
        format!(
            "Closest reference group: {}; next group is {:.1} log-likelihood units behind. Only {markers} markers were used; this is rough.",
            best.population,
            ancestry.get(1).map_or(0.0, |x| x.difference)
        )
    } else {
        "No ancestry similarity result.".into()
    };
    SampleChecks {
        sex: SexReport {
            classification: sex.into(),
            y_reads_per_million: y,
            control_reads_per_million: control,
            evidence: "XY: Y > control and >1,000 reads/million. XX: Y <5% of control and control >1,000 reads/million. Otherwise unclear; this is not a sex or identity determination.".into(),
        },
        telomere: TelomereReport {
            telomeric_reads: m.telomeric,
            reads_per_million: m.telomeric as f64 / reads * 1e6,
            length_adjusted_index: m.telomeric as f64 / reads * 1e6 * 150.0
                / qc.mean_len.max(1.0),
            message: "DNA sequence cannot tell your age; this is a rough research measure of telomere DNA, which shortens with age on average".into(),
        },
        ancestry,
        ancestry_summary,
    }
}
pub fn ancestry(panel: &Panel, calls: &[GenotypeCall]) -> Vec<AncestryScore> {
    let mut out = Vec::new();
    for population in ["AFR", "AMR", "EAS", "EUR", "SAS"] {
        let mut log_likelihood = 0.0;
        let mut markers = 0;
        for (variant, call) in panel.variants.iter().zip(calls) {
            if !call.called {
                continue;
            }
            let Some(freq) = variant.frequencies.get(population) else {
                continue;
            };
            let copies = call.genotype.split('/').collect::<Vec<_>>();
            let frequency =
                |allele: &str| freq.get(allele).copied().unwrap_or(0.01).clamp(0.01, 0.99);
            let probability = if copies.len() == 1 {
                frequency(copies[0])
            } else {
                let f0 = frequency(copies[0]);
                let f1 = frequency(copies[1]);
                if copies[0] == copies[1] {
                    f0 * f0
                } else {
                    2.0 * f0 * f1
                }
            };
            log_likelihood += probability.max(1e-12).ln();
            markers += 1;
        }
        out.push(AncestryScore {
            population: population.into(),
            log_likelihood,
            difference: 0.0,
            markers,
        });
    }
    let best = out
        .iter()
        .map(|x| x.log_likelihood)
        .fold(f64::NEG_INFINITY, f64::max);
    for score in &mut out {
        score.difference = best - score.log_likelihood;
    }
    out.sort_by(|a, b| a.difference.total_cmp(&b.difference));
    out
}
