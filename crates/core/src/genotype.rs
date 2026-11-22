use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenotypeCall {
    pub rsid: String,
    pub genotype: String,
    pub dosage: Option<u8>,
    pub depth: u32,
    pub gq: u8,
    pub ref_votes: u32,
    pub alt_votes: Vec<u32>,
    pub risk_allele: Option<String>,
    pub reference_allele: String,
    pub called: bool,
    pub no_call_reason: Option<String>,
    pub heteroplasmy: Option<f64>,
    #[serde(default)]
    pub gene: String,
    #[serde(default)]
    pub trait_group: String,
    #[serde(default)]
    pub evidence: Vec<helixscan_panel::Phenotype>,
    #[serde(default)]
    pub apoe: Option<String>,
}
fn dose(c: &GenotypeCall, allele: &str, v: &helixscan_panel::Variant) -> u8 {
    let i = if allele == v.ref_allele {
        0
    } else {
        v.alts
            .iter()
            .position(|a| a == allele)
            .map(|i| i + 1)
            .unwrap_or(usize::MAX)
    };
    if i == usize::MAX {
        return 0;
    }
    let selected = if i == 0 {
        c.ref_votes
    } else {
        c.alt_votes.get(i - 1).copied().unwrap_or(0)
    };
    let rest = c.depth.saturating_sub(selected);
    if selected == 0 {
        0
    } else if rest == 0 {
        2
    } else {
        1
    }
}
#[allow(clippy::too_many_lines)]
pub fn call(v: &helixscan_panel::Variant, counts: &[u32; 4], sex: &str) -> GenotypeCall {
    let r = counts[0];
    let alts = v
        .alts
        .iter()
        .enumerate()
        .map(|(i, _)| counts[(i + 1).min(3)])
        .collect::<Vec<_>>();
    let a = alts.iter().sum::<u32>();
    let d = r + a;
    let risk = v.phenotypes.iter().find_map(|p| p.risk_allele.clone());
    let mut base = GenotypeCall {
        rsid: v.rsid.clone(),
        genotype: "no call".into(),
        dosage: None,
        depth: d,
        gq: 0,
        ref_votes: r,
        alt_votes: alts.clone(),
        risk_allele: risk,
        reference_allele: v.ref_allele.clone(),
        called: false,
        no_call_reason: Some("low coverage".into()),
        heteroplasmy: None,
        gene: v.gene.clone(),
        trait_group: v.trait_group.clone(),
        evidence: v.phenotypes.clone(),
        apoe: None,
    };
    if v.chr == "MT" {
        if d == 0 {
            return base;
        }
        base.called = true;
        base.no_call_reason = None;
        base.heteroplasmy = Some(a as f64 / d as f64 * 100.0);
        let best = (0..alts.len()).max_by_key(|i| alts[*i]).unwrap_or(0);
        base.genotype = if r > alts[best] {
            v.ref_allele.clone()
        } else {
            v.alts[best].clone()
        };
        return base;
    }
    if d < 8 {
        return base;
    }
    let best_alt = (0..alts.len()).max_by_key(|i| alts[*i]).unwrap_or(0);
    let alt = &v.alts[best_alt];
    let aa = alts[best_alt];
    let e = 0.01f64;
    let logs = [
        r as f64 * (1.0 - e).ln() + aa as f64 * e.ln(),
        -(d as f64) * 2f64.ln(),
        r as f64 * e.ln() + aa as f64 * (1.0 - e).ln(),
    ];
    let best = (0..3).max_by(|x, y| logs[*x].total_cmp(&logs[*y])).unwrap();
    let max = logs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let sum = logs.iter().map(|x| (x - max).exp()).sum::<f64>();
    let best_p = (logs[best] - max).exp() / sum;
    base.gq = (-10f64 * (1.0 - best_p).max(1e-10).log10())
        .round()
        .clamp(0.0, 99.0) as u8;
    base.called = true;
    base.no_call_reason = None;
    if sex == "XY" && (v.chr == "X" || v.chr == "Y") {
        base.genotype = if r >= aa {
            v.ref_allele.clone()
        } else {
            alt.clone()
        };
        base.gq = if d == 0 {
            0
        } else {
            (-10.0 * ((r.min(aa) as f64 + 0.5) / (d as f64 + 1.0)).log10()).clamp(0.0, 99.0) as u8
        };
    } else {
        base.genotype = match best {
            0 => format!("{0}/{0}", v.ref_allele),
            1 => format!("{}/{}", v.ref_allele, alt),
            _ => format!("{0}/{0}", alt),
        };
    }
    if sex == "XY" && (v.chr == "X" || v.chr == "Y") {
        base.dosage = base
            .risk_allele
            .as_ref()
            .map(|risk| u8::from(base.genotype.split('/').any(|allele| allele == risk)));
    } else {
        base.dosage = base.risk_allele.as_ref().map(|risk| dose(&base, risk, v));
    }
    base
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApoeCall {
    pub haplotypes: Vec<String>,
    pub label: String,
}
pub fn apoe(panel: &helixscan_panel::Panel, calls: &[GenotypeCall]) -> ApoeCall {
    let get = |id: &str| calls.iter().find(|c| c.rsid == id);
    let (Some(a), Some(b)) = (get("rs429358"), get("rs7412")) else {
        return ApoeCall {
            haplotypes: vec![],
            label: "unknown".into(),
        };
    };
    if !a.called || !b.called {
        return ApoeCall {
            haplotypes: vec![],
            label: "unknown".into(),
        };
    }
    let mut haplotypes = Vec::new();
    for h in &panel.apoe_haplotypes {
        let n = alleles(&a.genotype)
            .iter()
            .filter(|x| **x == h.rs429358)
            .count()
            .min(
                alleles(&b.genotype)
                    .iter()
                    .filter(|x| **x == h.rs7412)
                    .count(),
            );
        for _ in 0..n {
            haplotypes.push(h.name.clone());
        }
    }
    haplotypes.sort();
    let label = if haplotypes.len() == 2 {
        haplotypes.join("/")
    } else {
        "complex/unknown".into()
    };
    ApoeCall { haplotypes, label }
}
fn alleles(g: &str) -> Vec<&str> {
    g.split('/').collect()
}
