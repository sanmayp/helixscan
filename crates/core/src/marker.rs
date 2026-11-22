use crate::encode::SimdLevel;
use crate::{Panel, genotype::GenotypeCall};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

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
    dense: OnceLock<MarkerLookup>,
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
        Self {
            y,
            control,
            x,
            dense: OnceLock::new(),
        }
    }
    /// Build a custom marker index; its compact cache remains lazy and internal.
    pub fn from_sets(y: HashSet<u64>, control: HashSet<u64>, x: HashSet<u64>) -> Self {
        Self {
            y,
            control,
            x,
            dense: OnceLock::new(),
        }
    }
    fn lookup(&self) -> &MarkerLookup {
        self.dense.get_or_init(|| MarkerLookup::new(self))
    }
}

/// A cache-local prefix filter preceding an exact full-key open-addressed table.
#[derive(Debug)]
struct MarkerLookup {
    prefixes: Vec<u8>,
    keys: Vec<u64>,
    flags: Vec<u8>,
    mask: usize,
}
impl MarkerLookup {
    fn new(index: &MarkerIndex) -> Self {
        let count = index.y.len() + index.control.len() + index.x.len();
        let capacity = (count.max(1) * 2).next_power_of_two();
        let mut this = Self {
            prefixes: vec![0; 1 << 24],
            keys: vec![0; capacity],
            flags: vec![0; capacity],
            mask: capacity - 1,
        };
        for (flag, keys) in [(1u8, &index.y), (2, &index.control), (4, &index.x)] {
            for &key in keys {
                let mixed = key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                this.prefixes[(mixed >> 40) as usize] |= flag;
                let mut slot = mixed as usize & this.mask;
                while this.keys[slot] != 0 {
                    if this.keys[slot] == key + 1 {
                        this.flags[slot] |= flag;
                        break;
                    }
                    slot = (slot + 1) & this.mask;
                }
                if this.keys[slot] == 0 {
                    this.keys[slot] = key + 1;
                    this.flags[slot] = flag;
                }
            }
        }
        this
    }
    #[inline]
    fn flags(&self, key: u64) -> u8 {
        let mixed = key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        if self.prefixes[(mixed >> 40) as usize] == 0 {
            return 0;
        }
        let mut slot = mixed as usize & self.mask;
        for _ in 0..=self.mask {
            if self.keys[slot] == key + 1 {
                return self.flags[slot];
            }
            if self.keys[slot] == 0 {
                return 0;
            }
            slot = (slot + 1) & self.mask;
        }
        0
    }
}

/// Observe one canonical k-mer and preserve per-read boolean evidence.
pub fn observe_marker_key(
    index: &MarkerIndex,
    key: crate::encode::KmerKey,
    y: &mut bool,
    control: &mut bool,
) {
    *y |= index.y.contains(&key.canonical);
    if index.x.contains(&key.canonical) {
        // X-homolog matches are evidence against Y specificity, even if
        // a read happens to contain both marker and homolog sequence.
        *y = false;
    }
    *control |= index.control.contains(&key.canonical);
}

pub struct MarkerScanner {
    pub index: Arc<MarkerIndex>,
    encoder: crate::encode::KmerEncoder,
}
impl MarkerScanner {
    pub fn new(index: Arc<MarkerIndex>, level: SimdLevel) -> Self {
        Self {
            index,
            encoder: crate::encode::KmerEncoder::new(level),
        }
    }
    /// Check each marker at most once per read, even when several k-mers match.
    pub fn add_keys(&mut self, seq: &[u8], local: &mut MarkerLocal) {
        let mut y = false;
        let mut control = false;
        let lookup = self.index.lookup();
        self.encoder.for_each_key(seq, |key| {
            let flags = lookup.flags(key.canonical);
            y |= flags & 1 != 0;
            if flags & 4 != 0 {
                y = false;
            }
            control |= flags & 2 != 0;
        });
        local.y += u64::from(y);
        local.control += u64::from(control);
    }
    pub fn add(&mut self, seq: &[u8], local: &mut MarkerLocal) {
        self.add_telomere(seq, local);
        self.add_keys(seq, local);
    }
    pub fn add_telomere(&mut self, seq: &[u8], local: &mut MarkerLocal) {
        if has_telomere(seq) {
            local.telomeric += 1;
        }
    }
    #[inline]
    pub fn observe(&self, key: crate::encode::KmerKey, y: &mut bool, control: &mut bool) {
        let flags = self.index.lookup().flags(key.canonical);
        *y |= flags & 1 != 0;
        if flags & 4 != 0 {
            // X-homolog matches still suppress Y specificity after a Y hit.
            *y = false;
        }
        *control |= flags & 2 != 0;
    }
}
const fn repeated_motif(motif: &[u8; 6]) -> u128 {
    let mut code = 0u128;
    let mut repeat = 0;
    while repeat < 7 {
        let mut i = 0;
        while i < 6 {
            let base = match motif[i] {
                b'A' => 0,
                b'C' => 1,
                b'G' => 2,
                b'T' => 3,
                _ => 0,
            };
            code = (code << 2) | base;
            i += 1;
        }
        repeat += 1;
    }
    code
}
const TELOMERE_FORWARD: u128 = repeated_motif(b"TTAGGG");
const TELOMERE_REVERSE: u128 = repeated_motif(b"CCCTAA");

/// A 42-base rolling window replaces twelve whole-read restart scans.
fn has_telomere(seq: &[u8]) -> bool {
    let mut code = 0u128;
    let mut valid = 0usize;
    for &base in seq {
        let Some(base) = crate::base_encode(base) else {
            code = 0;
            valid = 0;
            continue;
        };
        code = ((code << 2) | u128::from(base)) & ((1u128 << 84) - 1);
        valid += 1;
        if valid >= 42 && (code == TELOMERE_FORWARD || code == TELOMERE_REVERSE) {
            return true;
        }
    }
    false
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

#[cfg(test)]
mod telomere_tests {
    #[test]
    fn single_pass_matches_restart_scan_at_every_boundary() {
        let mut state = 19u64;
        for length in 0..260 {
            let mut seq = Vec::with_capacity(length);
            for i in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(14_465_269_304_089_633_739);
                seq.push(if (state >> 60) > 12 {
                    b"Nnacgt"[(state >> 58) as usize % 6]
                } else {
                    b"ACGTacgtTTAGGCC"[(state >> 58) as usize % 15]
                });
                if i % 37 == 0 {
                    seq.extend_from_slice(if i % 74 == 0 { b"TTAGGGT" } else { b"CCCTAAC" });
                }
            }
            for motif in [b"TTAGGG", b"CCCTAA"] {
                let other = if motif == b"TTAGGG" {
                    b"CCCTAA"
                } else {
                    b"TTAGGG"
                };
                assert_eq!(
                    super::has_telomere(&seq),
                    super::tandem(&seq, motif) >= 7 || super::tandem(&seq, other) >= 7
                );
            }
            // Every insertion of an exact seven-repeat motif must be found in
            // both directions; lowercase and non-ACGT bytes are tested above.
            for motif in [b"TTAGGG".repeat(7), b"CCCTAA".repeat(7)] {
                let mut seq = b"ACGTNNacgt".repeat(12);
                for offset in 0..seq.len() {
                    let mut with = seq.clone();
                    with.splice(offset..offset, motif.iter().copied());
                    assert!(super::has_telomere(&with));
                }
                seq.truncate(41);
                assert!(!super::has_telomere(&seq));
            }
            for repeats in 0..9 {
                let seq = b"TTAGGG".repeat(repeats);
                assert_eq!(super::has_telomere(&seq), repeats >= 7);
                let seq = b"CCCTAA".repeat(repeats);
                assert_eq!(super::has_telomere(&seq), repeats >= 7);
            }
        }
    }
}
