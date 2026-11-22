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
