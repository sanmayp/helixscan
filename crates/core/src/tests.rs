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
