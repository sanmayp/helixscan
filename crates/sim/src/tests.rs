use crate::*;
use helixscan_panel::{Panel, Variant};
use rand::SeedableRng;

fn panel() -> Panel {
    Panel {
        schema_version: 1,
        reference: "GRCh38".into(),
        ensembl_release: 116,
        fetched_at: "test".into(),
        pgx_rules: String::new(),
        searched_traits: vec![],
        marker_regions: vec![],
        apoe_haplotypes: vec![],
        variants: vec![Variant {
            rsid: "rs1".into(),
            gene: "TEST".into(),
            trait_group: "Test".into(),
            section: "health".into(),
            note: "".into(),
            allele_offset: 0,
            raw_ref: "A".into(),
            raw_alts: vec!["C".into()],
            chr: "1".into(),
            pos: 100,
            end: 100,
            reference: "A".into(),
            ref_allele: "A".into(),
            alts: vec!["C".into()],
            flank_left: "ACGT".repeat(15),
            flank_right: "TGCA".repeat(15),
            phenotypes: vec![],
            effect_allele: None,
            effect_allele_status: helixscan_panel::EffectAlleleStatus::Unclear,
            effect_findings: vec![],
            clinical_significance: vec![],
            frequencies: Default::default(),
            fetched_at: "test".into(),
            ensembl_release: 116,
        }],
    }
}
#[test]
fn reverses_both_sides() {
    let mut s = b"ACGTN".to_vec();
    reverse_complement(&mut s);
    assert_eq!(s, b"NACGT");
}
#[test]
fn chunks_reproduce_in_any_order() {
    let p = panel();
    let c = Config {
        seed: 42,
        depth: 2,
        read_len: 80,
        population: None,
        sex: Sex::Xx,
        telomeric_fraction: 0.0,
    };
    let t = truths(&p, &c);
    let a = chunk_reads(&p, 8, 8, &c, &t);
    let b = chunk_reads(&p, 8, 8, &c, &t);
    assert_eq!(a, b);
}
#[test]
fn parallel_chunks_preserve_order() {
    let p = panel();
    let c = Config {
        seed: 42,
        depth: 2,
        read_len: 80,
        population: None,
        sex: Sex::Xx,
        telomeric_fraction: 0.0,
    };
    let t = truths(&p, &c);
    let mut parallel = Vec::new();
    parallel_fastq(&mut parallel, &p, 20_000, &c, &t).unwrap();
    let mut sequential = Vec::new();
    for start in [0usize, 8192, 16384] {
        sequential.extend_from_slice(&chunk_reads(
            &p,
            start as u64,
            8192.min(20_000 - start),
            &c,
            &t,
        ));
    }
    assert_eq!(parallel, sequential);
}
#[test]
fn fastq_records_well_formed() {
    let p = panel();
    let c = Config {
        seed: 7,
        depth: 2,
        read_len: 80,
        population: None,
        sex: Sex::Xx,
        telomeric_fraction: 0.0,
    };
    let t = truths(&p, &c);
    let mut v = Vec::new();
    parallel_fastq(&mut v, &p, 20, &c, &t).unwrap();
    let s = String::from_utf8(v).unwrap();
    let lines: Vec<_> = s.lines().collect();
    assert_eq!(lines.len(), 80);
    for r in lines.as_chunks::<4>().0 {
        assert!(r[0].starts_with('@'));
        assert_eq!(r[1].len(), r[3].len());
        assert!(r[2].starts_with('+'));
    }
}
#[test]
fn substitutions_are_deterministic() {
    let mut a = rand::rngs::SmallRng::seed_from_u64(4);
    let mut b = rand::rngs::SmallRng::seed_from_u64(4);
    assert_eq!(
        make_record(&mut a, 1, &mut vec![], &mut vec![], 100),
        make_record(&mut b, 1, &mut vec![], &mut vec![], 100)
    );
}
