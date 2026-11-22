use crate::{
    Panel,
    genotype::{ApoeCall, GenotypeCall, apoe},
    qc::Qc,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub disclaimer: String,
    pub engine: String,
    pub elapsed_seconds: f64,
    pub qc: Qc,
    pub variants: Vec<GenotypeCall>,
    pub apoe: ApoeCall,
    pub sample_checks: crate::marker::SampleChecks,
    pub traits: Vec<TraitReport>,
    pub pgx: Vec<PgxReport>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraitReport {
    pub section: String,
    pub trait_group: String,
    pub rsids: Vec<String>,
    pub message: String,
    pub evidence: Vec<helixscan_panel::Phenotype>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PgxReport {
    pub gene: String,
    pub phenotype: String,
    pub consult: String,
}
pub fn report(
    panel: &Panel,
    calls: Vec<GenotypeCall>,
    qc: Qc,
    engine: String,
    elapsed_seconds: f64,
    sample_checks: crate::marker::SampleChecks,
) -> Report {
    let mut traits = Vec::new();
    let mut groups = std::collections::BTreeMap::new();
    for (v, c) in panel.variants.iter().zip(&calls) {
        groups
            .entry((v.section.clone(), v.trait_group.clone()))
            .or_insert_with(Vec::new)
            .push((v, c));
    }
    for ((section, trait_group), entries) in groups {
        let found = entries
            .iter()
            .filter(|(_, c)| c.called && c.alt_votes.iter().sum::<u32>() > 0)
            .count();
        let mut message=match section.as_str(){"cancer"|"carrier"=>if found>0{format!("Found {} panel variant(s). Confirm with a clinical genetic test and a genetic counselor before any decision.",found)}else{format!("Not found among the {} variants checked. This does NOT rule out risk: many other variants in these genes are not checked.",entries.len())},"ancestry"=>"Ancestry similarity only; these few markers are rough and do not establish identity.".into(),"health"=>"Common-variant associations only; this does not predict whether you will get a disease.".into(),"trait"=>"An observed panel genotype, not a guarantee of a personal characteristic.".into(),_=>"See the checked variants and their Ensembl evidence.".into()};
        if section == "health" {
            let risk = entries.iter().find(|(_, c)| {
                c.called && c.risk_allele.is_some() && c.dosage.is_some_and(|d| d > 0)
            });
            if let Some((v, c)) = risk {
                message = format!(
                    "The {} genotype at {} is associated with this trait in Ensembl evidence; common variants generally have small effects.",
                    c.genotype, v.rsid
                );
            }
        }
        let evidence = entries
            .iter()
            .flat_map(|(v, _)| v.phenotypes.clone())
            .collect();
        traits.push(TraitReport {
            section,
            trait_group,
            rsids: entries.iter().map(|(v, _)| v.rsid.clone()).collect(),
            message,
            evidence,
        });
    }
    for searched in &panel.searched_traits {
        traits.push(TraitReport {
            section: searched.section.clone(),
            trait_group: searched.trait_group.clone(),
            rsids: searched.markers.clone(),
            message: searched.message.clone(),
            evidence: vec![],
        });
    }
    let pgx = crate::marker::pgx_interpretations(panel, &calls);
    Report{schema_version:1,disclaimer:"Research and education only. Not a medical diagnosis. Talk to a doctor or genetic counselor about any result.".into(),engine,elapsed_seconds,qc,apoe:apoe(panel,&calls),variants:calls,traits,pgx,sample_checks,limitations:vec!["Your age".into(),"Blood pressure".into(),"Whether you will get a disease".into(),"Colour blindness".into(),"CYP2D6 type".into(),"Full HLA type".into(),"Rare variants outside the panel".into()]}
}
pub fn text_report(r: &Report) -> String {
    format!(
        "{}\n\nReads: {}\nPanel coverage: {:.1}%\n\n{}\n",
        r.disclaimer, r.qc.reads, r.qc.panel_coverage, r.sample_checks.telomere.message
    )
}
