use crate::{Panel, genotype::GenotypeCall};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SampleChecks {
    pub sex: SexReport,
    pub telomere: TelomereReport,
    pub ancestry: Vec<AncestryScore>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SexReport {
    pub classification: String,
    pub y_reads_per_million: f64,
    pub control_reads_per_million: f64,
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
    pub fn add(&mut self, seq: &[u8]) {
        if tandem(seq, b"TTAGGG") >= 7 || tandem(seq, b"CCCTAA") >= 7 {
            self.telomeric += 1;
        }
    }
}
fn tandem(seq: &[u8], motif: &[u8]) -> usize {
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
    let reads = qc.reads.max(1) as f64;
    let y = m.y as f64 / reads * 1e6;
    let c = m.control as f64 / reads * 1e6;
    let sex = if y > c * 2.0 && y > 10.0 {
        "XY"
    } else if y < 2.0 && c > 1.0 {
        "XX"
    } else {
        "unclear"
    };
    let ancestry = ancestry(panel, calls);
    SampleChecks{sex:SexReport{classification:sex.into(),y_reads_per_million:y,control_reads_per_million:c},telomere:TelomereReport{telomeric_reads:m.telomeric,reads_per_million:m.telomeric as f64/reads*1e6,length_adjusted_index:m.telomeric as f64/reads*1e6*150.0/qc.mean_len.max(1.0),message:"DNA sequence cannot tell your age; this is a rough research measure of telomere DNA, which shortens with age on average".into()},ancestry}
}
pub fn ancestry(panel: &Panel, calls: &[GenotypeCall]) -> Vec<AncestryScore> {
    let mut out = Vec::new();
    for pop in ["AFR", "AMR", "EAS", "EUR", "SAS"] {
        let mut ll = 0.0;
        for (v, c) in panel.variants.iter().zip(calls) {
            if !c.called {
                continue;
            }
            let Some(freq) = v.frequencies.get(pop) else {
                continue;
            };
            let genotype = c.genotype.split('/').collect::<Vec<_>>();
            let p = if genotype.len() == 1 {
                0.5
            } else {
                let f = |a: &str| {
                    if a == v.ref_allele {
                        freq.get(&v.ref_allele).copied().unwrap_or(0.01)
                    } else {
                        freq.get(a).copied().unwrap_or(0.01)
                    }
                };
                let f0 = f(genotype[0]).clamp(0.01, 0.99);
                let f1 = f(genotype[1]).clamp(0.01, 0.99);
                if genotype[0] == genotype[1] {
                    f0 * f0
                } else {
                    2.0 * f0 * f1
                }
            };
            ll += p.max(1e-12).ln();
        }
        out.push(AncestryScore {
            population: pop.into(),
            log_likelihood: ll,
            difference: 0.0,
        });
    }
    let best = out
        .iter()
        .map(|x| x.log_likelihood)
        .fold(f64::NEG_INFINITY, f64::max);
    for x in &mut out {
        x.difference = best - x.log_likelihood;
    }
    out
}
pub fn pgx_interpretations(panel: &Panel, calls: &[GenotypeCall]) -> Vec<crate::report::PgxReport> {
    let mut results = Vec::new();
    for gene in [
        "CYP2C19",
        "CYP2C9/VKORC1",
        "SLCO1B1",
        "DPYD",
        "TPMT/NUDT15",
        "CYP3A5",
        "HCP5",
    ] {
        let text = if gene == "CYP2C19" {
            let loss = call_alt(panel, calls, "rs4244285") + call_alt(panel, calls, "rs4986893");
            let rapid = call_alt(panel, calls, "rs12248560");
            if loss >= 2 {
                "CYP2C19 poor metabolizer"
            } else if loss == 1 {
                "CYP2C19 intermediate metabolizer"
            } else if rapid >= 2 {
                "CYP2C19 ultrarapid metabolizer"
            } else if rapid == 1 {
                "CYP2C19 rapid metabolizer"
            } else {
                "CYP2C19 normal metabolizer"
            }
        } else {
            match gene {
                "CYP2C9/VKORC1" => {
                    "The genotype may affect warfarin treatment; the prescriber uses an official dosing table."
                }
                "SLCO1B1" => {
                    if call_alt(panel, calls, "rs4149056") > 0 {
                        "Higher simvastatin muscle side-effect risk."
                    } else {
                        "The checked simvastatin-associated allele was not found; other risk factors remain."
                    }
                }
                "DPYD" => {
                    if call_alt(panel, calls, "rs3918290") > 0 {
                        "Reduced DPD and high fluorouracil toxicity risk; confirm before treatment."
                    } else {
                        "DPYD *2A was not found in this panel; other alleles are not excluded."
                    }
                }
                "TPMT/NUDT15" => {
                    "Interpret the checked TPMT/NUDT15 alleles before thiopurine treatment."
                }
                "CYP3A5" => "Interpret CYP3A5 *1 carrier status with the prescriber.",
                "HCP5" => {
                    if call_alt(panel, calls, "rs2395029") > 0 {
                        "Possible HLA-B*57:01 tag. Confirm with an HLA test before abacavir; this tag is imperfect."
                    } else {
                        "The HCP5 tag allele was not found; this is not a full HLA test."
                    }
                }
                _ => "No data interpretation",
            }
        };
        results.push(crate::report::PgxReport {
            gene: gene.into(),
            phenotype: text.into(),
            consult:
                "Show this to your doctor or pharmacist; do not change any medicine on your own."
                    .into(),
        });
    }
    results
}
fn call_alt(panel: &Panel, calls: &[GenotypeCall], id: &str) -> u8 {
    panel
        .variants
        .iter()
        .position(|v| v.rsid == id)
        .and_then(|i| calls.get(i))
        .filter(|c| c.called)
        .map(|c| c.alt_votes.iter().sum::<u32>().min(2) as u8)
        .unwrap_or(0)
}
