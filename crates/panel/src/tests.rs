#[cfg(test)]
mod panel_tests {
    use crate::{PanelBuilder, Rest, Value, allele_context, normalize_alleles, parse_seed};
    use std::path::PathBuf;

    #[test]
    fn normalized_indels_realign_exactly() {
        let (off, r, a) = normalize_alleles("TCTT", &["T".into(), "TCTTCTT".into()]).unwrap();
        assert_eq!(
            (
                off,
                r.as_str(),
                a.iter().map(String::as_str).collect::<Vec<_>>()
            ),
            (1, "CTT", vec!["", "CTTCTT"])
        );
        let (off, r, a) = normalize_alleles("GGG", &["GGGG".into()]).unwrap();
        assert_eq!((off, r, a), (3, "".into(), vec!["G".into()]));
        let (off, r, a) = normalize_alleles("A", &["AC".into()]).unwrap();
        assert_eq!((off, r, a), (1, "".into(), vec!["C".into()]));
    }
    #[test]
    fn context_replaces_the_normalized_event() {
        let mut v: crate::Variant = serde_json::from_value(serde_json::json!({"rsid":"rs","gene":"G","trait_group":"T","section":"health","note":"","allele_offset":1,"chr":"7","pos":1,"end":4,"reference":"TCTT","ref":"CTT","alts":["", "CTTCTT"],"flank_left":"A","flank_right":"C","phenotypes":[],"clinical_significance":[],"frequencies":{},"fetched_at":"","ensembl_release":1})).unwrap();
        assert_eq!(allele_context(&v, "CTT"), (b"ATCTTC".to_vec(), 2));
        assert_eq!(allele_context(&v, ""), (b"ATC".to_vec(), 2));
        assert_eq!(allele_context(&v, "CTTCTT"), (b"ATCTTCTTC".to_vec(), 2));
        v.ref_allele = "".into();
        v.reference = "GGG".into();
        v.flank_left = "A".into();
        v.flank_right = "C".into();
        v.allele_offset = 3;
        assert_eq!(allele_context(&v, "G"), (b"AGGGGC".to_vec(), 4));
    }
    #[test]
    fn parse_all_seed_columns() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/panel.seed.tsv");
        let rows = parse_seed(&path).unwrap();
        assert_eq!(rows.len(), 65);
        assert_eq!(rows[0].expected_gene, "TCF7L2");
        assert!(rows.iter().any(|v| v.rsid == "rs113993960"));
    }
    #[test]
    fn apoe_rules_from_panel_data() {
        use anyhow::anyhow;
        use serde_json::json;
        struct Fake;
        impl Rest for Fake {
            fn json(&self, path: &str) -> anyhow::Result<Value> {
                match path {
                    "info/data?" => Ok(json!({"releases":[116]})),
                    p if p.starts_with("variation/human/") => Ok(
                        json!({"mappings":[{"assembly_name":"GRCh38","coord_system":"chromosome","start":100,"end":100,"seq_region_name":"19","allele_string":"T/C"}],"clinical_significance":[],"phenotypes":[{"trait":"Alzheimer disease","risk_allele":"C","pvalue":"1e-10","study":"PMID:1","source":"test"}]}),
                    ),
                    p if p.starts_with("overlap/region/") => {
                        Ok(json!([{"external_name":"APOE","start":100,"end":100}]))
                    }
                    p if p.starts_with("sequence/region/") => {
                        Ok(json!({"seq":format!("{}T{}", "A".repeat(60), "C".repeat(60))}))
                    }
                    _ => Err(anyhow!("unexpected")),
                }
            }
            fn post(&self, path: &str, ids: &[String]) -> anyhow::Result<Value> {
                assert_eq!(path, "variation/human?pops=1");
                assert_eq!(ids, ["rs429358"]);
                Ok(json!({}))
            }
        }
        let dir = std::env::temp_dir();
        let p = dir.join(format!("helixscan-panel-test-{}", std::process::id()));
        std::fs::write(
            &p,
            "health\tAlzheimer (APOE type)\tAPOE\trs429358\talzheimer\t\n",
        )
        .unwrap();
        let panel = PanelBuilder::new(Fake).build(&p, 1).unwrap();
        std::fs::remove_file(p).unwrap();
        assert_eq!(panel.variants[0].reference, "T");
        assert_eq!(
            panel.variants[0].phenotypes[0].pmid.as_deref(),
            Some("PMID:1")
        );
        assert_eq!(panel.apoe_haplotypes.len(), 3);
    }
}
