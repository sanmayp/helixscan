use helixscan_panel::{EffectAlleleStatus, Panel, Phenotype, Variant, audit_panel, effect_audit};

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
    assert!(result.effect_findings.is_empty());
}

#[test]
fn real_77_variant_panel_selects_tcf7l2_t_and_audits_every_variant() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let panel: Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    assert_eq!(panel.variants.len(), 77);
    assert_eq!(
        panel
            .variants
            .iter()
            .filter(|v| v.effect_allele_status == EffectAlleleStatus::Unclear)
            .count(),
        73
    );
    let v = panel
        .variants
        .iter()
        .find(|v| v.rsid == "rs7903146")
        .unwrap();
    assert_eq!(effect_audit(v).effect_allele.as_deref(), Some("T"));
    assert_eq!(v.effect_allele.as_deref(), Some("T"));
    assert_eq!(v.effect_allele_status, EffectAlleleStatus::Supported);
    assert!(panel.variants.iter().all(|v| {
        if v.effect_allele_status == EffectAlleleStatus::Supported {
            v.effect_allele.is_some()
        } else {
            v.effect_allele.is_none() && v.effect_findings.is_empty()
        }
    }));
    let mut rebuilt = panel.clone();
    rebuilt.variants.iter_mut().for_each(|v| {
        v.effect_allele = None;
        v.effect_allele_status = EffectAlleleStatus::Unclear;
        v.effect_findings.clear();
    });
    audit_panel(&mut rebuilt);
    assert_eq!(
        serde_json::to_value(panel).unwrap(),
        serde_json::to_value(rebuilt).unwrap()
    );
}
