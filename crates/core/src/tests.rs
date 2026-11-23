use helixscan_panel::{Haplotype, Variant};
pub fn variant(rsid: &str, reference: &str, alt: &str) -> Variant {
    Variant {
        rsid: rsid.into(),
        gene: "TEST".into(),
        trait_group: "Test".into(),
        section: "health".into(),
        note: String::new(),
        allele_offset: 0,
        raw_ref: reference.into(),
        raw_alts: vec![alt.into()],
        chr: "1".into(),
        pos: 100,
        end: 100,
        reference: reference.into(),
        ref_allele: reference.into(),
        alts: vec![alt.into()],
        flank_left: "ACGTTGCA".repeat(8),
        flank_right: "TGCACGTA".repeat(8),
        phenotypes: vec![],
        effect_allele: None,
        effect_allele_status: helixscan_panel::EffectAlleleStatus::Unclear,
        curated_effect: None,
        effect_findings: vec![],
        clinical_significance: vec![],
        frequencies: Default::default(),
        fetched_at: "test".into(),
        ensembl_release: 1,
    }
}
pub fn panel(v: Variant) -> helixscan_panel::Panel {
    helixscan_panel::Panel {
        schema_version: 1,
        reference: "GRCh38".into(),
        ensembl_release: 1,
        fetched_at: "test".into(),
        pgx_rules: String::new(),
        searched_traits: vec![],
        marker_regions: vec![],
        variants: vec![v],
        apoe_haplotypes: Vec::<Haplotype>::new(),
    }
}
#[test]
fn encoding_rejects_ambiguous() {
    assert!(crate::kmer(&[b'A'; 31]).is_some());
    assert!(crate::kmer(&[b'N'; 31]).is_none());
    assert_eq!(
        crate::encode::encode(b"ACGTNacgtn", crate::encode::SimdLevel::Scalar),
        crate::encode::encode(b"ACGTNacgtn", crate::encode::SimdLevel::Avx512)
    );
}
#[test]
fn snp_read_vote_and_reverse_complement() {
    let v = variant("rs1", "A", "C");
    let index = crate::PanelIndex::new(&panel(v.clone()));
    let (s, _) = crate::allele_context(&v, "C");
    let seq = s[30..75].to_vec();
    let q = vec![35; seq.len()];
    assert_eq!(index.vote(&seq, &q), Some((0, 1)));
    let rc = crate::reverse_complement(&seq);
    assert_eq!(index.vote(&rc, &q), Some((0, 1)));
}
#[test]
fn low_quality_variant_position_rejects_vote() {
    let v = variant("rs1", "A", "C");
    let index = crate::PanelIndex::new(&panel(v.clone()));
    let (s, pos) = crate::allele_context(&v, "C");
    let seq = s[30..75].to_vec();
    let mut q = vec![35; seq.len()];
    q[pos - 30] = 10;
    assert_eq!(index.vote(&seq, &q), None);
}
#[test]
fn shared_marker_traversal_preserves_per_read_evidence_and_votes() {
    use std::sync::Arc;
    let p = panel(variant("rs1", "A", "C"));
    let (seq, _) = crate::allele_context(&p.variants[0], "C");
    let qual = vec![35; seq.len()];
    let random = |(offset, length): (usize, usize)| {
        (0..length)
            .map(|i| b"ACGT"[(offset + i * 7 + i / 3 + i / 11) % 4])
            .collect::<Vec<_>>()
    };
    let y = random((0, 31)).repeat(7);
    let control = random((61, 31)).repeat(7);
    let x = random((107, 31)).repeat(7);
    let mixed = [
        b"TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT".as_slice(),
        y.as_slice(),
        b"TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT".as_slice(),
        control.as_slice(),
        b"TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT".as_slice(),
        x.as_slice(),
        b"TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT".as_slice(),
    ]
    .concat();
    let mixed_qual = vec![35; mixed.len()];
    let y_key = crate::kmer(&y[..31]).unwrap();
    let control_key = crate::kmer(&control[..31]).unwrap();
    let x_key = crate::kmer(&x[..31]).unwrap();
    assert_ne!(y_key, control_key);
    assert_ne!(y_key, x_key);
    assert_ne!(control_key, x_key);
    let index = Arc::new(crate::marker::MarkerIndex::from_sets(
        [y_key].into_iter().collect(),
        [control_key].into_iter().collect(),
        [x_key].into_iter().collect(),
    ));
    let panel_index = crate::PanelIndex::new(&p);
    for level in [
        crate::encode::SimdLevel::Scalar,
        crate::encode::SimdLevel::Avx2,
        crate::encode::SimdLevel::Avx512,
    ] {
        if !level.available() {
            continue;
        }
        let mut scanner = crate::marker::MarkerScanner::new(index.clone(), level);
        let mut one = crate::marker::MarkerLocal::default();
        scanner.add(&y, &mut one);
        assert_eq!(
            (one.y, one.control),
            (1, 0),
            "multiple Y k-mers contribute only once to this read"
        );
        let mut mixed_only = crate::marker::MarkerLocal::default();
        scanner.add(&mixed, &mut mixed_only);
        assert_eq!((mixed_only.y, mixed_only.control), (0, 1));
        let mut expected = crate::marker::MarkerLocal::default();
        expected.merge(&one);
        expected.merge(&mixed_only);
        let mut separate_votes = vec![[0u32; 4]; p.variants.len()];
        let mut encoder = crate::encode::KmerEncoder::new(level);
        let mut hits = Vec::new();
        crate::index::scan_read_with(
            &panel_index,
            &seq,
            &qual,
            &mut separate_votes,
            &mut encoder,
            &mut hits,
        );
        let mut shared_votes = vec![[0u32; 4]; p.variants.len()];
        let mut combined = crate::marker::MarkerLocal::default();
        for read in [&y[..], &mixed[..], &seq[..]] {
            let qual = if read.len() == seq.len() {
                &qual[..]
            } else {
                &mixed_qual[..read.len()]
            };
            crate::index::scan_read_with_markers(
                &panel_index,
                (read, qual),
                &mut shared_votes,
                &mut encoder,
                &mut hits,
                &mut scanner,
                &mut combined,
            );
        }
        assert_eq!(expected.y, 1, "one per qualifying read, not per k-mer");
        assert_eq!(expected.control, 1);
        assert_eq!(combined.y, expected.y);
        assert_eq!(combined.control, expected.control);
        assert_eq!(shared_votes, separate_votes);
    }
}

#[test]
fn filtered_variant_votes_match_full_vote_logic() {
    let v = variant("rs1", "A", "C");
    let index = crate::PanelIndex::new(&panel(v.clone()));
    for allele in ["A", "C"] {
        let (seq, _) = crate::allele_context(&v, allele);
        let qual = vec![35; seq.len()];
        let reverse = crate::reverse_complement(&seq);
        for read in [&seq[..], &reverse[..]] {
            let mut encoder = crate::encode::KmerEncoder::new(index.simd);
            let mut hits = Vec::new();
            index.read_hits_with(read, &mut encoder, &mut hits);
            assert_eq!(
                index.vote_variant_hits(read, &qual, 0, &hits),
                index
                    .vote_hits(read, &qual, &hits)
                    .map(|(_, allele)| allele)
            );
        }
    }
}

#[test]
fn insertion_and_deletion_reads_vote() {
    let mut del = variant("del", "T", "CTT");
    del.reference = "TCTT".into();
    del.ref_allele = "T".into();
    del.alts = vec!["".into()];
    del.allele_offset = 3;
    del.flank_left = "ACGTTGCAACGGATCGTACCTAGGATCCGATGCATCGTAGCTAGGCTAACGTTACGGATCCGTA".into();
    del.flank_right =
        "TGCACGTAGCATTCGATGCCATTACGGTACCTGACTACCGGTATTCAGCGATCCGTAGCATGCATTCGGA".into();
    let p = helixscan_panel::Panel {
        variants: vec![del.clone()],
        ..panel(variant("unused", "A", "C"))
    };
    let index = crate::PanelIndex::new(&p);
    for (a, allele) in [(0, "T"), (1, "")] {
        let (s, pos) = crate::allele_context(&del, allele);
        let seq = s[pos - 40..pos + 65].to_vec();
        assert_eq!(index.vote(&seq, &vec![35; seq.len()]), Some((0, a)));
    }
}
#[test]
fn likelihood_prefers_heterozygote() {
    let v = variant("rs1", "A", "C");
    let c = crate::genotype::call(&v, &[20, 20, 0, 0], "XX");
    assert_eq!(c.genotype, "A/C");
    assert!(c.gq > 10);
}

#[test]
fn multiallelic_calls_retain_both_alt_alleles() {
    let mut v = variant("rs6420484", "A", "C");
    v.alts = vec!["C".into(), "G".into(), "T".into()];
    // REF A=0, ALT C=0, ALT G=20, ALT T=0. The old caller invented A/C.
    let c = crate::genotype::call(&v, &[0, 0, 20, 0], "XX");
    assert_eq!(c.genotype, "G/G");
    assert_eq!(c.depth, 20);
    // A genuine ALT/ALT genotype must not be represented as REF/ALT.
    let c = crate::genotype::call(&v, &[0, 10, 10, 0], "XX");
    assert_eq!(c.genotype, "C/G");
    assert_eq!(c.depth, 20);
}
#[test]
fn parses_fastq_with_at_quality() {
    let fastq = b"@r\nACGT\n+\n@!!!\n@r2\nACGT\n+\n!!!!\n";
    let mut reader = crate::fastq::FastqReader::new(std::io::Cursor::new(fastq), 65536);
    let b = reader.next_batch().unwrap().unwrap();
    assert_eq!(b.records.len(), 2);
    assert!(reader.next_batch().unwrap().is_none());
}

#[test]
fn real_panel_simulated_truth_agreement() {
    #[derive(serde::Deserialize)]
    struct TruthFile {
        variants: Vec<Truth>,
    }
    #[derive(serde::Deserialize)]
    struct Truth {
        rsid: String,
        genotype: String,
        copies: Vec<String>,
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let panel: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let truth: TruthFile = serde_json::from_reader(
        std::fs::File::open(root.join("testdata/small.truth.json")).unwrap(),
    )
    .unwrap();
    let result =
        crate::scan::scan_path(&root.join("testdata/small.fastq"), &panel, 2, None).unwrap();
    assert_eq!(result.qc.reads, 6500);
    let mismatches = result
        .calls
        .iter()
        .zip(&truth.variants)
        .filter(|(c, t)| {
            c.rsid == t.rsid
                && c.called
                && !c
                    .genotype
                    .split('/')
                    .all(|a| t.copies.contains(&a.to_string()))
        })
        .map(|(c, t)| {
            (
                t.rsid.clone(),
                t.genotype.clone(),
                c.genotype.clone(),
                c.depth,
            )
        })
        .collect::<Vec<_>>();
    let called = result.calls.iter().filter(|c| c.called).count();
    let agreement = 100.0 * (called - mismatches.len()) as f64 / called as f64;
    eprintln!(
        "called {called}/{} agreement {agreement:.3}% mismatches {mismatches:?}",
        panel.variants.len()
    );
    assert!(agreement >= 99.5);
}

#[test]
fn fastq_block_boundaries_preserve_records() {
    let fastq = b"@r\nACGT\n+\n@!!!\n@r2\nACGT\n+\n!!!!\n";
    for size in 1..fastq.len() {
        let mut reader = crate::fastq::FastqReader::new(std::io::Cursor::new(fastq), size);
        let mut reads = 0;
        while let Some(batch) = reader.next_batch().unwrap() {
            reads += batch.records.len();
        }
        assert_eq!(reads, 2, "block size {size}");
    }
}

fn simulated_sample(
    sex: helixscan_sim::Sex,
    population: Option<&str>,
    telomere: f64,
) -> crate::scan::ScanResult {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let panel: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let config = helixscan_sim::Config {
        seed: 71,
        depth: 20,
        read_len: 150,
        population: population.map(str::to_string),
        sex,
        telomeric_fraction: telomere,
    };
    let truth = helixscan_sim::truths(&panel, &config);
    let path = root.join(format!(
        "testdata/check-{sex:?}-{}-{telomere}-{:?}.fastq",
        population.unwrap_or("random"),
        std::thread::current().id()
    ));
    helixscan_sim::parallel_fastq(
        std::fs::File::create(&path).unwrap(),
        &panel,
        20_000,
        &config,
        &truth,
    )
    .unwrap();
    let result = crate::scan::scan_path(&path, &panel, 2, None).unwrap();
    if std::env::var_os("HELIXSCAN_KEEP_FIXTURE").is_none() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[test]
fn simulated_xy_and_xx_marker_reads_classify() {
    for (sex, expected) in [
        (helixscan_sim::Sex::Xy, "XY"),
        (helixscan_sim::Sex::Xx, "XX"),
    ] {
        let result = simulated_sample(sex, None, 0.0);
        assert_eq!(result.checks.sex.classification, expected);
        assert!(result.checks.sex.control_reads_per_million > 1_000.0);
        if expected == "XY" {
            assert!(
                result.checks.sex.y_reads_per_million > result.checks.sex.control_reads_per_million
            );
        } else {
            assert!(
                result.checks.sex.y_reads_per_million
                    < result.checks.sex.control_reads_per_million * 0.05
            );
        }
    }
}

#[test]
fn simulated_telomeres_are_counted_at_seven_repeats() {
    let result = simulated_sample(helixscan_sim::Sex::Xx, None, 0.2);
    assert!(result.checks.telomere.telomeric_reads > 500);
    assert!(result.checks.telomere.length_adjusted_index > 100_000.0);
    assert!(
        result
            .checks
            .telomere
            .message
            .contains("cannot tell your age")
    );
    assert_eq!(
        crate::marker::tandem(b"TTAGGGTTAGGGTTAGGGTTAGGGTTAGGGTTAGGGTTAGGG", b"TTAGGG"),
        7
    );
}

#[test]
fn simulated_population_frequency_draws_score_all_groups() {
    let result = simulated_sample(helixscan_sim::Sex::Xx, Some("AFR"), 0.0);
    assert_eq!(result.checks.ancestry.len(), 5);
    assert!(result.checks.ancestry_summary.contains("this is rough"));
    assert!(result.checks.ancestry[0].markers > 10);
    assert!(result.checks.ancestry[0].difference.abs() < 1e-12);
    // All called alleles were drawn from the selected population's frequencies.
    assert!(result.checks.ancestry[0].log_likelihood.is_finite());
    assert!(
        result
            .checks
            .ancestry
            .iter()
            .all(|group| group.markers > 10)
    );
}

#[test]
fn simulated_pgx_calls_apply_versioned_rules() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = simulated_sample(helixscan_sim::Sex::Xx, None, 0.0);
    let panel: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let output = crate::pgx::pgx_interpretations(&panel, &result.calls);
    assert!(!output.is_empty());
    assert!(output.iter().all(|item| item.gene != "Rule-data error"));
    assert!(
        output
            .iter()
            .all(|item| item.consult.contains("pharmacist"))
    );
    assert!(
        output
            .iter()
            .find(|item| item.gene == "CYP2C19")
            .is_some_and(|item| item.phenotype.contains("metabolizer"))
    );
}

#[test]
fn pgx_low_coverage_is_not_a_normal_call() {
    let panel = panel(variant("rs4244285", "G", "A"));
    let call = crate::genotype::call(&panel.variants[0], &[0; 4], "unclear");
    let output = crate::pgx::pgx_interpretations(&panel, &[call]);
    assert!(output[0].phenotype.contains("could not be evaluated"));
}

#[test]
fn real_panel_tcf7l2_effect_allele_is_t_and_unclear_never_counts_copies() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let v = p.variants.iter().find(|v| v.rsid == "rs7903146").unwrap();
    assert_eq!(v.alts, ["G", "T"]);
    let call = crate::genotype::call(v, &[0, 0, 20, 0], "XX");
    assert_eq!(call.risk_allele.as_deref(), Some("T"));
    assert_eq!(call.dosage, Some(2));
    assert_eq!(call.genotype, "T/T");

    let mut unclear = v.clone();
    unclear.effect_allele_status = helixscan_panel::EffectAlleleStatus::Unclear;
    let call = crate::genotype::call(&unclear, &[0, 0, 20, 0], "XX");
    assert_eq!(call.risk_allele, None);
    assert_eq!(call.dosage, None);
    assert_eq!(call.genotype, "T/T");
}

#[test]
fn sensitive_reports_and_limitations_are_data_derived() {
    let mut v = variant("rs1", "A", "C");
    v.section = "carrier".into();
    let p = panel(v.clone());
    let call = crate::genotype::call(&v, &[20, 0, 0, 0], "unclear");
    let qc = crate::qc::Qc {
        reads: 1,
        panel_coverage: 100.0,
        ..Default::default()
    };
    let report = crate::report::report(&p, vec![call], qc, "test".into(), 0.0, Default::default());
    assert!(
        report.traits[0]
            .message
            .contains("This does NOT rule out risk")
    );
    assert!(report.limitations.iter().any(|x| x == "CYP2D6 type"));
    assert!(report.disclaimer.contains("Not a medical diagnosis"));
}

#[test]
fn effect_allele_population_shares_use_hardy_weinberg_not_genotype_inference() {
    let mut v = variant("rs-frequency", "C", "T");
    v.effect_allele = Some("T".into());
    v.effect_allele_status = helixscan_panel::EffectAlleleStatus::Supported;
    v.frequencies.insert(
        "ALL".into(),
        [("C".into(), 0.75), ("T".into(), 0.25)].into(),
    );
    v.frequencies
        .insert("AFR".into(), [("C".into(), 1.0)].into());
    let call = crate::genotype::call(&v, &[10, 10, 0, 0], "XX");
    let frequencies = call.population_frequencies.as_ref().unwrap();
    let all = frequencies[0].as_ref().unwrap();
    assert_eq!(all.population, "ALL");
    assert_eq!(all.allele_frequency, 0.25);
    assert_eq!(all.zero_copies, 0.5625);
    assert_eq!(all.one_copy, 0.375);
    assert_eq!(all.two_copies, 0.0625);
    assert!(frequencies[1].is_none(), "absence is not evidence of zero");
    assert_eq!(frequencies.len(), 6);
    let json = serde_json::to_value(&call).unwrap();
    assert_eq!(json["population_frequencies"][0]["zero_copies"], 0.5625);
    assert_eq!(json["population_frequencies"][1], serde_json::Value::Null);

    // Saved-report JSON must preserve the exact computed shares.
    v.frequencies
        .insert("EAS".into(), [("T".into(), 0.0228174603174603)].into());
    let call = crate::genotype::call(&v, &[10, 10, 0, 0], "XX");
    let json = serde_json::to_value(&call).unwrap();
    let encoded = serde_json::to_string(&call).unwrap();
    let restored: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, json);

    v.frequencies.clear();
    let frequencies = crate::genotype::call(&v, &[10, 10, 0, 0], "XX")
        .population_frequencies
        .unwrap();
    assert_eq!(frequencies.len(), 6);
    assert!(frequencies.iter().all(Option::is_none));
}

/// One genotype/dosage/JSON expectation for each embedded curated row.
#[test]
fn every_curated_row_produces_exact_marker_dosage_and_provenance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    for row in helixscan_panel::curated_effects().unwrap() {
        let v = p.variants.iter().find(|v| v.rsid == row.rsid).unwrap();
        let allele = &v.curated_effect.as_ref().unwrap().effect_allele;
        let mut counts = [0; 4];
        if allele == &v.ref_allele {
            counts[0] = 20;
        } else {
            counts[v.alts.iter().position(|a| a == allele).unwrap() + 1] = 20;
        }
        let call = crate::genotype::call(v, &counts, "XX");
        assert_eq!(call.genotype, format!("{allele}/{allele}"), "{}", row.rsid);
        assert_eq!(call.dosage, Some(2), "{}", row.rsid);
        assert_eq!(call.risk_allele.as_deref(), Some(allele.as_str()));
        assert_eq!(
            call.effect_allele_status,
            helixscan_panel::EffectAlleleStatus::Curated
        );
        let json = serde_json::to_value(&call).unwrap();
        assert_eq!(json["effect_allele_status"], "curated");
        assert_eq!(json["dosage"], 2);
        assert_eq!(json["curated_effect"], serde_json::to_value(&row).unwrap());
        let no_call = crate::genotype::call(v, &[0; 4], "XX");
        assert!(!no_call.called && no_call.dosage.is_none());
    }
}

#[test]
fn curated_dosage_uses_the_called_allele_not_all_other_read_votes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    let f5 = p.variants.iter().find(|v| v.rsid == "rs6025").unwrap();
    assert_eq!(f5.ref_allele, "C");
    assert_eq!(f5.alts, ["A", "T"]);
    assert_eq!(f5.effect_allele.as_deref(), Some("T"));
    let c = crate::genotype::call(f5, &[10, 10, 0, 0], "XX");
    assert_eq!(c.genotype, "C/A");
    assert_eq!(c.dosage, Some(0));
    let c = crate::genotype::call(f5, &[0, 10, 10, 0], "XX");
    assert_eq!(c.genotype, "A/T");
    assert_eq!(c.dosage, Some(1));
    assert_eq!(c.alt_votes, [10, 10]);
    assert_eq!(c.depth, 20);

    let cftr = p.variants.iter().find(|v| v.rsid == "rs113993960").unwrap();
    let c = crate::genotype::call(cftr, &[10, 10, 0, 0], "XX");
    assert_eq!(c.genotype, "CTT/");
    assert_eq!(c.dosage, Some(1));
    assert_eq!(c.risk_allele.as_deref(), Some(""));
    let c = crate::genotype::call(cftr, &[20, 0, 0, 0], "XX");
    assert_eq!(c.genotype, "CTT/CTT");
    assert_eq!(c.dosage, Some(0));

    let ref_effect = p.variants.iter().find(|v| v.rsid == "rs776746").unwrap();
    assert_eq!(
        ref_effect.effect_allele.as_deref(),
        Some(ref_effect.ref_allele.as_str())
    );
    let c = crate::genotype::call(ref_effect, &[20, 0, 0, 0], "XX");
    assert_eq!(c.genotype, "T/T");
    assert_eq!(c.dosage, Some(2));
    let c = crate::genotype::call(ref_effect, &[0, 20, 0, 0], "XX");
    assert_eq!(c.genotype, "C/C");
    assert_eq!(c.dosage, Some(0));
}

#[test]
fn curated_health_summary_names_its_source_not_ensembl_gwas() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut p: crate::Panel =
        serde_json::from_reader(std::fs::File::open(root.join("data/panel.json")).unwrap())
            .unwrap();
    p.variants.retain(|v| v.rsid == "rs7903146");
    let call = crate::genotype::call(&p.variants[0], &[0, 0, 20, 0], "XX");
    let r = crate::report::report(
        &p,
        vec![call],
        crate::qc::Qc::default(),
        "test".into(),
        0.0,
        Default::default(),
    );
    let message = &r
        .traits
        .iter()
        .find(|t| t.trait_group == "Type 2 diabetes")
        .unwrap()
        .message;
    assert!(message.contains("Source: PMID:17463248"));
    assert!(message.contains("effect-marker copy/copies"));
    assert!(!message.contains("in Ensembl evidence"));
}

#[test]
fn confidence_uses_both_depth_and_gq_and_never_upgrades_no_calls() {
    use crate::genotype::call_confidence;
    for (depth, gq, called, expected) in [
        (0, 0, false, "low"),
        (100, 99, false, "low"),
        (7, 99, true, "low"),
        (8, 0, true, "medium"),
        (19, 99, true, "medium"),
        (20, 29, true, "medium"),
        (20, 30, true, "high"),
        (100, 99, true, "high"),
    ] {
        assert_eq!(call_confidence(depth, gq, called), expected);
    }

    let p = panel(variant("rs-confidence", "A", "C"));
    let high = crate::genotype::call(&p.variants[0], &[20, 0, 0, 0], "XX");
    assert_eq!(high.confidence, "high");
    assert_eq!(serde_json::to_value(&high).unwrap()["confidence"], "high");
    let no_call = crate::genotype::call(&p.variants[0], &[0; 4], "XX");
    assert_eq!(no_call.confidence, "low");
    let mixed = crate::genotype::call(&p.variants[0], &[5, 5, 0, 0], "XX");
    assert!(mixed.called && mixed.depth < 20);
    assert_eq!(mixed.confidence, "medium");
}
