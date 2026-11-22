use anyhow::{Result, anyhow};
use helixscan_panel::{PanelBuilder, Rest, Variant};
use serde_json::{Value, json};

/// Recorded from POST https://rest.ensembl.org/variation/human?pops=1 with
/// {"ids":["rs7903146","rs113993960"]}; only requested 1000G rows are kept.
struct RecordedPost;
impl Rest for RecordedPost {
    fn json(&self, path: &str) -> Result<Value> {
        match path {
            "info/data?" => Ok(json!({"releases": [116]})),
            p if p.starts_with("variation/human/rs113993960?") => Ok(json!({
                "mappings": [{"assembly_name":"GRCh38", "coord_system":"chromosome",
                    "start":100, "end":103, "seq_region_name":"7", "allele_string":"TCTT/T"}],
                "clinical_significance":[], "phenotypes":[]
            })),
            p if p.starts_with("variation/human/") => Ok(json!({
                "mappings": [{"assembly_name":"GRCh38", "coord_system":"chromosome",
                    "start":100, "end":100, "seq_region_name":"10", "allele_string":"C/T"}],
                "clinical_significance":[], "phenotypes":[]
            })),
            p if p.starts_with("overlap/region/human/7:") => {
                Ok(json!([{"external_name":"CFTR", "start":100, "end":103}]))
            }
            p if p.starts_with("overlap/region/human/10:") => {
                Ok(json!([{"external_name":"TCF7L2", "start":100, "end":100}]))
            }
            p if p.starts_with("sequence/region/human/7:") => {
                Ok(json!({"seq":format!("{}TCTT{}", "A".repeat(60), "C".repeat(60))}))
            }
            p if p.starts_with("sequence/region/human/10:") => {
                Ok(json!({"seq":format!("{}C{}", "A".repeat(60), "C".repeat(60))}))
            }
            _ => Err(anyhow!("unexpected GET in fixture test: {path}")),
        }
    }

    fn post(&self, path: &str, ids: &[String]) -> Result<Value> {
        assert_eq!(path, "variation/human?pops=1");
        assert_eq!(ids, ["rs113993960", "rs7903146"]);
        Ok(serde_json::from_str(include_str!(
            "fixtures/ensembl_population_post.json"
        ))?)
    }
}

#[test]
fn batched_post_stores_all_six_populations_in_panel_coordinates() {
    let seed = std::env::temp_dir().join(format!("h1c-panel-{}.tsv", std::process::id()));
    std::fs::write(
        &seed,
        "health\tType 2 diabetes\tTCF7L2\trs7903146\t.\t\n\
carrier\tCystic fibrosis\tCFTR\trs113993960\t.\t\n",
    )
    .unwrap();
    let panel = PanelBuilder::new(RecordedPost).build(&seed, 0).unwrap();
    std::fs::remove_file(seed).unwrap();
    assert_eq!(panel.variants.len(), 2);
    let snp: &Variant = &panel.variants[0];
    let indel: &Variant = &panel.variants[1];
    for (variant, effect, frequency) in [
        (snp, "T", 0.227835463258786),
        (indel, "", 0.00399361022364217),
    ] {
        assert_eq!(variant.effect_allele.as_deref(), Some(effect));
        assert_eq!(variant.frequencies.len(), 6);
        assert_eq!(variant.frequencies["ALL"][effect], frequency);
        for population in ["ALL", "AFR", "AMR", "EAS", "EUR", "SAS"] {
            assert!(variant.frequencies.contains_key(population));
        }
    }
    assert_eq!(indel.frequencies["EUR"][""], 0.00894632206759443);
    assert!(!indel.frequencies["AFR"].contains_key(""));
}
