use crate::{EffectAlleleStatus, Panel, effect_audit};

/// Deterministic audit of all variants, including every explicit curated override.
pub fn effect_report(panel: &Panel) -> String {
    let supported = panel
        .variants
        .iter()
        .filter(|v| v.effect_allele_status == EffectAlleleStatus::Supported)
        .count();
    let curated = panel
        .variants
        .iter()
        .filter(|v| v.effect_allele_status == EffectAlleleStatus::Curated)
        .count();
    let mut report = format!(
        "# Effect-allele audit\n\n\
         Source snapshot: `data/panel.json`, Ensembl release {}, plus H0c `data/curated_effects.toml` version 1. This audit covers **all {} variants**. Original Ensembl `risk_allele` values and read-derived genotypes have not been overwritten.\n\n\
         ## Statuses and dosage\n\n\
         - `supported`: unique allele from the conservative GWAS rule below.\n\
         - `curated`: an exact rsID/gene/REF-or-ALT match from the small, versioned hand-curated table. Curated rows replace either `unclear` or the generic GWAS result. Each genotype call repeats the row as `curated_effect` and has `effect_allele_status = \"curated\"`, so JSON carries its direction, plain meaning and source.\n\
         - `unclear`: no selected effect allele; `dosage = null`. Every non-curated variant keeps its original GWAS-only supported/unclear disposition. A curated row does not turn nearby unlisted rsIDs or other ALT alleles into supported effects.\n\n\
         Dosage is a count of the specifically named effect allele, **not** a disease probability, diagnosis or medication dose. A beneficial or functional allele can be counted just as a susceptibility allele can. For haplotypes and medication interpretation, read the row meaning plus the APOE/PGx results, not dosage alone.\n\n\
         ## Conservative GWAS rule\n\n\
         Only findings whose `trait_text` equals the card's `trait_group` (case-insensitive) are eligible. Of those, only an odds ratio strictly greater than 1 or a beta containing `unit increase` carries an increasing-risk direction. Decreasing effects, other beta units, absent effect measures, ClinVar labels without an effect measure, gene names and allele frequencies do **not** select an effect allele. Multiple eligible alleles, a missing eligible allele, or an allele absent from the panel make it `unclear`.\n\n\
         No eligible odds ratios are recorded in this Ensembl snapshot; retained alleles therefore come from own-trait beta `unit increase` findings. Trait labels such as `Heart and cholesterol` are not literal Ensembl traits, so no finding qualifies for those cards under this rule. The H0c table is a deliberately separate provenance layer and does not rewrite those findings.\n\n\
         Evidence remains available for inspection but does not itself imply direction or clinical validity. This is not an estimate of personal risk.\n\n\
         ## Curated overrides\n\n\
         All alleles below are GRCh38 forward-strand alleles as represented by the panel, not necessarily bases on the gene transcript. Source identifiers refer to ClinVar, PharmGKB/CPIC guidance, or the key paper. ClinVar records can be opened as `https://www.ncbi.nlm.nih.gov/clinvar/variation/<numeric-VCV-id>/`; PMIDs as `https://pubmed.ncbi.nlm.nih.gov/<PMID>/`. Gene/drug guidance is indexed at <https://www.clinpgx.org/cpic/pairs> and <https://www.pharmgkb.org/>.\n\n\
         | Variant | Gene | Effect allele | Direction | Plain meaning | Source |\n\
         |---|---|---|---|---|---|\n",
        panel.ensembl_release,
        panel.variants.len()
    );
    for v in &panel.variants {
        if let Some(row) = &v.curated_effect {
            assert_eq!(v.effect_allele_status, EffectAlleleStatus::Curated);
            assert_eq!(v.effect_allele.as_ref(), Some(&row.effect_allele));
            report.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                v.rsid,
                row.gene,
                if row.effect_allele.is_empty() {
                    "(deletion)"
                } else {
                    &row.effect_allele
                },
                escape(&row.effect_direction),
                escape(&row.plain_meaning),
                escape(&row.source)
            ));
        }
    }
    report.push_str(
        "\n### Curation boundaries\n\n\
         rs6025 is **T**, not A, on the GRCh38 forward strand: T corresponds to the reverse-strand F5 transcript's Leiden c.1601G>A substitution. Panel ALT A is not Leiden.\n\n\
         APOE e4 requires rs429358 C **and** rs7412 C; rs429358 T with rs7412 T defines e2. The two rows carry haplotype wording, and dosage does not replace the existing two-site APOE result.\n\n\
         The CYP2C19, CYP2C9 and CYP3A5 marker counts are not diplotypes or prescribing instructions. Existing versioned PGx evaluation remains a separate path; its allele identities must be checked independently when this table changes. CPIC/PharmGKB references are labels for reviewer lookup, not a claim that generic dosage implements those guidelines.\n\n\
         HFE H63D is included as a well-known allele-definition row but ClinVar classifications conflict; its row expressly avoids claiming inevitable disease. CFTR F508del's normalized effect allele is the empty string (the three-base deletion), which `Some(\"\")` deliberately distinguishes from `None`/unclear.\n\n\
         ## Variant-by-variant GWAS audit\n\n\
         `old` is the first finding allele used by the old caller. `new` is the final panel allele. GWAS counts exclude the separate curated table.\n\n\
         | Variant | Gene | Card trait_group | Status | Old | New | Own-trait findings / eligible | Reason |\n\
         |---|---|---|---|---|---|---:|---|\n",
    );
    for v in &panel.variants {
        let a = effect_audit(v);
        let reason = if let Some(row) = &v.curated_effect {
            format!(
                "**Curated override**: {}. Source: {}.",
                row.effect_direction, row.source
            )
        } else {
            match a.status {
                EffectAlleleStatus::Supported if a.old_allele == a.effect_allele => format!(
                    "Unchanged: unique supported allele; eligible evidence: {}.",
                    evidence(&a.effect_findings)
                ),
                EffectAlleleStatus::Supported => format!(
                    "**Changed by H0a**: the old first-finding allele was not a supported increasing-risk allele; unique eligible allele; evidence: {}.",
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
                EffectAlleleStatus::Curated => unreachable!("pure GWAS audit never emits curated"),
            }
        };
        let status = match v.effect_allele_status {
            EffectAlleleStatus::Supported => "supported",
            EffectAlleleStatus::Curated => "**curated**",
            EffectAlleleStatus::Unclear => "**unclear**",
        };
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {}/{} | {} |\n",
            v.rsid,
            v.gene,
            escape(&v.trait_group),
            status,
            a.old_allele.as_deref().unwrap_or("none"),
            v.effect_allele.as_deref().unwrap_or("**unclear**"),
            a.own_trait_findings,
            a.qualifying_findings,
            escape(&reason),
        ));
    }
    report.push_str(&format!(
        "\n## Outcome\n\n\
         {supported} variants have a GWAS-supported effect allele, {curated} are **curated**, and {} remain **unclear** with no dosage. Before H0c, 4 were supported and 73 unclear. The underlying GWAS alleles remain unchanged; this includes H0a's changes at **rs7903146 (C → T)** and **rs1111875 (G → C)**. All {} JSON effect statuses are covered by exact table/invariant tests. Genotypes, alternative alleles, read depths and truth-check concordance are not changed by curated annotations.\n\n\
         ## Reproduction\n\n\
         Run `cargo run -p helixscan-panel -- audit-effects`. Rust embeds and validates `data/curated_effects.toml`, rewrites the effect fields in `data/panel.json`, and regenerates this document. No Python, JavaScript or network service evaluates effects. The tests cover every row plus decreasing, disagreeing, wrong-trait, missing-measure, unsupported-allele and OR boundary cases.\n",
        panel.variants.len() - supported - curated,
        panel.variants.len(),
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
