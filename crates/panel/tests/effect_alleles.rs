use helixscan_panel::{
    EffectAlleleStatus, Panel, Phenotype, Variant, audit_panel, audit_panel_checked, effect_audit,
};

fn finding(trait_text: &str, allele: &str, beta: Option<&str>, odds: Option<&str>) -> Phenotype {
    Phenotype {
        trait_text: trait_text.into(),
        risk_allele: Some(allele.into()),
        p_value: None,
        pmid: Some("PMID:35551307".into()),
        source: Some("NHGRI-EBI GWAS catalog".into()),
        odds_ratio: odds.map(str::to_string),
        beta_coefficient: beta.map(str::to_string),
    }
}
fn variant() -> Variant {
    serde_json::from_value(serde_json::json!({
        "rsid":"rs7903146", "gene":"TCF7L2", "trait_group":"Type 2 diabetes",
        "section":"health", "note":"", "allele_offset":0, "chr":"10",
        "pos":112998590, "end":112998590, "reference":"C", "ref":"C",
        "alts":["G", "T"], "flank_left":"A", "flank_right":"C",
        "phenotypes":[], "clinical_significance":[], "frequencies":{},
        "fetched_at":"", "ensembl_release":116
    }))
    .unwrap()
}
fn real_panel() -> Panel {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap()).unwrap()
}

#[test]
fn decreasing_first_finding_cannot_select_the_protective_allele() {
    let mut v = variant();
    v.phenotypes = vec![
        finding("Type 2 diabetes", "C", Some("0.28 unit decrease"), None),
        finding("Type 2 diabetes", "T", Some("0.228 unit increase"), None),
    ];
    let result = effect_audit(&v);
    assert_eq!(result.old_allele.as_deref(), Some("C"));
    assert_eq!(result.effect_allele.as_deref(), Some("T"));
    assert_eq!(result.status, EffectAlleleStatus::Supported);
}

#[test]
fn trait_scope_effect_direction_and_disagreement_all_make_allele_unclear() {
    let mut v = variant();
    v.phenotypes
        .push(finding("Other trait", "C", None, Some("1.4")));
    assert_eq!(effect_audit(&v).status, EffectAlleleStatus::Unclear);

    v.phenotypes
        .push(finding("type 2 DIABETES", "T", Some("0.4 increase"), None));
    assert_eq!(effect_audit(&v).status, EffectAlleleStatus::Unclear);

    v.phenotypes
        .push(finding("Type 2 diabetes", "T", None, Some("1")));
    assert_eq!(effect_audit(&v).status, EffectAlleleStatus::Unclear);
    v.phenotypes.last_mut().unwrap().odds_ratio = Some("1.4".into());
    assert_eq!(effect_audit(&v).effect_allele.as_deref(), Some("T"));

    v.phenotypes
        .push(finding("Type 2 diabetes", "C", None, Some("1.2")));
    assert_eq!(effect_audit(&v).status, EffectAlleleStatus::Unclear);
    assert_eq!(effect_audit(&v).effect_allele, None);
    assert!(effect_audit(&v).effect_findings.is_empty());

    v.phenotypes
        .retain(|p| p.risk_allele.as_deref() != Some("C"));
    v.phenotypes
        .push(finding("Type 2 diabetes", "A", None, Some("1.2")));
    let result = effect_audit(&v);
    assert_eq!(result.status, EffectAlleleStatus::Unclear);
    assert_eq!(result.effect_allele, None);
    assert!(effect_audit(&v).effect_findings.is_empty());
}

/// One explicit expectation per TOML row: gene, forward-strand allele,
/// direction/meaning, allowed source provenance, and resulting JSON status.
#[test]
fn every_h0c_curated_row_matches_its_reviewed_variant_and_metadata() {
    let mut p = real_panel();
    audit_panel(&mut p);
    let expected: [(&str, &str, &str, &str, &str, &str); 18] = [
        (
            "TCF7L2",
            "rs7903146",
            "T",
            "type 2 diabetes",
            "susceptibility",
            "17463248",
        ),
        (
            "FTO",
            "rs9939609",
            "A",
            "type 2 diabetes",
            "body weight",
            "17434869",
        ),
        (
            "APOE",
            "rs429358",
            "C",
            "APOE e4",
            "haplotype",
            "VCV000017864",
        ),
        ("APOE", "rs7412", "T", "APOE e2", "haplotype", "7920638"),
        (
            "HFE",
            "rs1800562",
            "A",
            "hemochromatosis",
            "penetrance",
            "VCV000000009",
        ),
        (
            "HFE",
            "rs1799945",
            "G",
            "H63D",
            "other HFE allele",
            "VCV000000010",
        ),
        (
            "F5",
            "rs6025",
            "T",
            "thrombosis",
            "A, is not Leiden",
            "VCV000000642",
        ),
        (
            "F2",
            "rs1799963",
            "A",
            "thrombosis",
            "G20210A",
            "VCV000013310",
        ),
        (
            "MCM6",
            "rs4988235",
            "A",
            "lactase persistence",
            "persistence",
            "11788828",
        ),
        (
            "ALDH2",
            "rs671",
            "A",
            "alcohol flushing",
            "enzyme activity",
            "VCV000018390",
        ),
        (
            "CYP2C19",
            "rs4244285",
            "A",
            "*2 no-function",
            "reverse-strand",
            "CPIC",
        ),
        (
            "CYP2C19",
            "rs4986893",
            "A",
            "*3 no-function",
            "no-function",
            "CPIC",
        ),
        (
            "CYP2C19",
            "rs12248560",
            "T",
            "*17 increased-function",
            "haplotypes",
            "CPIC",
        ),
        (
            "CYP2C9",
            "rs1799853",
            "T",
            "*2 decreased-function",
            "Arg144Cys",
            "CPIC",
        ),
        (
            "CYP2C9",
            "rs1057910",
            "C",
            "*3 decreased-function",
            "Ile359Leu",
            "CPIC",
        ),
        (
            "SLCO1B1",
            "rs4149056",
            "C",
            "transporter function",
            "myopathy",
            "CPIC",
        ),
        (
            "CYP3A5",
            "rs776746",
            "T",
            "*1 normal-function",
            "not a drug dose",
            "CPIC",
        ),
        (
            "CFTR",
            "rs113993960",
            "",
            "Phe508del",
            "Carrier status",
            "VCV000007105",
        ),
    ];
    assert_eq!(expected.len(), 18);
    assert_eq!(
        helixscan_panel::curated_effects().unwrap().len(),
        expected.len()
    );
    for (gene, rsid, allele, direction, meaning, source) in expected {
        let v = p.variants.iter().find(|v| v.rsid == rsid).unwrap();
        assert_eq!(v.gene, gene, "gene for {rsid}");
        assert!(
            v.effect_allele_status == EffectAlleleStatus::Curated,
            "{rsid} must override unclear/GWAS status"
        );
        assert_eq!(
            v.effect_allele.as_deref(),
            Some(allele),
            "reviewed allele for {rsid}"
        );
        assert!(
            allele == v.ref_allele || v.alts.iter().any(|a| a == allele),
            "allele exists in panel for {rsid}"
        );
        let row = v.curated_effect.as_ref().unwrap();
        assert_eq!(row.gene, gene);
        assert_eq!(row.rsid, rsid);
        assert!(
            row.effect_direction.contains(direction),
            "direction for {rsid}"
        );
        assert!(
            row.plain_meaning.contains(meaning),
            "plain meaning for {rsid}"
        );
        assert!(row.source.contains(source), "source for {rsid}");
        assert!(!row.plain_meaning.is_empty() && !row.source.is_empty());
        let json = serde_json::to_value(v).unwrap();
        assert_eq!(json["effect_allele_status"], "curated");
        assert_eq!(json["curated_effect"], serde_json::to_value(row).unwrap());
    }
}

#[test]
fn non_curated_gwas_statuses_never_change_and_every_unclear_remains_unclear() {
    let mut p = real_panel();
    let gwas = p
        .variants
        .iter()
        .map(|v| {
            let audit = effect_audit(v);
            (
                v.rsid.clone(),
                audit.effect_allele,
                audit.status,
                audit.effect_findings,
            )
        })
        .collect::<Vec<_>>();
    audit_panel_checked(&mut p).unwrap();
    assert_eq!(
        p.variants
            .iter()
            .filter(|v| v.effect_allele_status == EffectAlleleStatus::Curated)
            .count(),
        18
    );
    assert_eq!(
        p.variants
            .iter()
            .filter(|v| v.effect_allele_status == EffectAlleleStatus::Supported)
            .count(),
        3
    );
    assert_eq!(
        p.variants
            .iter()
            .filter(|v| v.effect_allele_status == EffectAlleleStatus::Unclear)
            .count(),
        56
    );
    for (v, (rsid, allele, status, findings)) in p.variants.iter().zip(gwas) {
        assert_eq!(v.rsid, rsid);
        match v.effect_allele_status {
            EffectAlleleStatus::Curated => assert!(v.curated_effect.is_some()),
            _ => {
                assert_eq!(
                    v.effect_allele, allele,
                    "GWAS allele must survive for {rsid}"
                );
                assert_eq!(
                    v.effect_allele_status, status,
                    "non-curated status changed for {rsid}"
                );
                assert_eq!(v.effect_findings, findings);
                assert!(v.curated_effect.is_none());
            }
        }
        if v.effect_allele_status == EffectAlleleStatus::Unclear {
            assert!(v.effect_allele.is_none() && v.effect_findings.is_empty());
        }
    }
    if let Some(v) = p.variants.iter_mut().find(|v| v.rsid == "rs6025") {
        assert_ne!(
            v.effect_allele.as_deref(),
            Some("A"),
            "A is a distinct non-Leiden ALT"
        );
    }
    if let Some(v) = p.variants.iter_mut().find(|v| v.rsid == "rs113993960") {
        assert_eq!(
            v.alts[0], "",
            "normalized F508del must be distinguishable from unclear"
        );
    }
}

#[test]
fn panel_reaudit_is_idempotent_including_embedded_curation() {
    let p = real_panel();
    let mut rebuilt = p.clone();
    rebuilt.variants.iter_mut().for_each(|v| {
        v.effect_allele = None;
        v.effect_allele_status = EffectAlleleStatus::Unclear;
        v.effect_findings.clear();
        v.curated_effect = None;
    });
    audit_panel_checked(&mut rebuilt).unwrap();
    let first = serde_json::to_value(&rebuilt).unwrap();
    audit_panel_checked(&mut rebuilt).unwrap();
    assert_eq!(first, serde_json::to_value(&rebuilt).unwrap());
}
