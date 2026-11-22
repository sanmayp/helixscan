use crate::{EffectAlleleStatus, Panel, effect_audit};

/// Deterministic audit of all variants, not merely a hand-selected example.
pub fn effect_report(panel: &Panel) -> String {
    let supported = panel
        .variants
        .iter()
        .filter(|v| v.effect_allele_status == EffectAlleleStatus::Supported)
        .count();
    let mut report = format!(
        "# Effect-allele audit\n\n\
         Source snapshot: `data/panel.json`, Ensembl release {}. This audit covers **all {} variants**. \
         The original Ensembl `risk_allele` values have not been overwritten.\n\n\
         ## Conservative rule\n\n\
         Only findings whose `trait_text` equals the card's `trait_group` (case-insensitive) \
         are eligible. Of those, only an odds ratio strictly greater than 1 or a beta containing \
         `unit increase` carries an increasing-risk direction. Decreasing effects, other beta units, \
         absent effect measures, ClinVar labels without an effect measure, gene names and allele \
         frequencies do **not** select an effect allele. Multiple eligible alleles, a missing eligible \
         allele, or an allele absent from the panel make it **unclear**. Unclear means `dosage = null` \
         in the genotype report: no risk-allele copies are counted.\n\n\
         No eligible odds ratios are recorded in this Ensembl snapshot; retained alleles therefore come \
         from own-trait beta `unit increase` findings. Trait labels such as `Heart and cholesterol` are \
         not literal Ensembl traits, so no finding qualifies for those cards under this rule. APOE haplotypes \
         and the separately versioned PGx interpretations do not depend on this generic risk dosage.\n\n\
         The report column is explicitly labelled `Unclear` when no allele is selected. Evidence remains \
         available for inspection but does not imply an increasing effect. This is a provenance and \
         direction audit, not clinical validation or an estimate of personal risk.\n\n\
         ## Variant-by-variant result\n\n\
         `old` is the first finding allele used by the old caller. `new` is the audited allele.\n\n\
         | Variant | Gene | Card trait_group | Old | New | Own-trait findings / eligible | Reason |\n\
         |---|---|---|---|---|---:|---|\n",
        panel.ensembl_release,
        panel.variants.len()
    );
    for v in &panel.variants {
        let a = effect_audit(v);
        let reason = match a.status {
            EffectAlleleStatus::Supported if a.old_allele == a.effect_allele => format!(
                "Unchanged: unique supported allele; eligible evidence: {}.",
                evidence(&a.effect_findings)
            ),
            EffectAlleleStatus::Supported => format!(
                "**Changed**: the old first-finding allele was not a supported increasing-risk allele; \
                 unique eligible allele; evidence: {}.",
                evidence(&a.effect_findings)
            ),
            EffectAlleleStatus::Unclear if a.own_trait_findings == 0 =>
                "**Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent.".into(),
            EffectAlleleStatus::Unclear if a.qualifying_findings == 0 => format!(
                "**Unclear**: {} own-trait finding(s), none with OR &gt; 1 or beta `unit increase`.{}",
                a.own_trait_findings,
                if a.decreasing_evidence.is_empty() {
                    String::new()
                } else {
                    format!(" Explicit decreasing examples: {}.", a.decreasing_evidence.join("; "))
                }
            ),
            EffectAlleleStatus::Unclear =>
                "**Unclear**: eligible findings disagree or name an allele absent from the panel; evidence is not reused.".into(),
        };
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} | {}/{} | {} |\n",
            v.rsid,
            v.gene,
            escape(&v.trait_group),
            a.old_allele.as_deref().unwrap_or("none"),
            a.effect_allele.as_deref().unwrap_or("**unclear**"),
            a.own_trait_findings,
            a.qualifying_findings,
            escape(&reason),
        ));
    }
    report.push_str(&format!(
        "\n## Outcome\n\n\
         {} variants have a supported effect allele; {} are unclear and have no dosage. \
         Variant alleles changed at **rs7903146 (C → T)** and **rs1111875 (G → C)**. \
         Genotypes, alternative alleles and read depths are unchanged. This audit does not claim that \
         unselected markers have no biological or medication effect; it means this evidence snapshot does \
         not support generic increasing-risk allele counts under the stated rule.\n\n\
         ## Reproduction\n\n\
         Run `cargo run -p helixscan-panel -- audit-effects`. It rewrites the audit fields in \
         `data/panel.json` and regenerates this document. The tests construct decreasing, disagreeing, \
         wrong-trait, missing-measure, unsupported-allele and OR boundary cases, and require \
         rs7903146 = T in the actual 77-variant panel.\n",
        supported,
        panel.variants.len() - supported
    ));
    report
}
fn evidence(findings: &[crate::Phenotype]) -> String {
    findings
        .iter()
        .map(|p| {
            format!(
                "{} / {} / {}",
                p.beta_coefficient
                    .as_deref()
                    .or(p.odds_ratio.as_deref())
                    .unwrap_or("no effect"),
                p.pmid.as_deref().unwrap_or("no PMID"),
                p.source.as_deref().unwrap_or("no source")
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
fn escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}
