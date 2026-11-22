use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Qc {
    pub reads: u64,
    pub bases: u64,
    pub q30_bases: u64,
    pub gc_bases: u64,
    pub n_bases: u64,
    pub mean_len: f64,
    pub q30_pct: f64,
    pub gc_pct: f64,
    pub n_pct: f64,
    pub panel_coverage: f64,
    pub length_histogram: Vec<(u32, u64)>,
}
#[derive(Clone, Debug, Default)]
pub struct QcLocal {
    pub reads: u64,
    pub bases: u64,
    pub q30: u64,
    pub gc: u64,
    pub n: u64,
    pub lengths: std::collections::BTreeMap<u32, u64>,
}
impl QcLocal {
    pub fn add(&mut self, seq: &[u8], qual: &[u8]) {
        self.reads += 1;
        self.bases += seq.len() as u64;
        *self.lengths.entry(seq.len() as u32).or_default() += 1;
        for (&b, &q) in seq.iter().zip(qual) {
            if q >= 30 {
                self.q30 += 1;
            }
            match b | 32 {
                b'g' | b'c' => self.gc += 1,
                b'n' => self.n += 1,
                _ => {}
            }
        }
    }
    pub fn merge(&mut self, other: &Self) {
        self.reads += other.reads;
        self.bases += other.bases;
        self.q30 += other.q30;
        self.gc += other.gc;
        self.n += other.n;
        for (k, v) in &other.lengths {
            *self.lengths.entry(*k).or_default() += v;
        }
    }
    pub fn finish(&self, called: usize, total: usize) -> Qc {
        Qc {
            reads: self.reads,
            bases: self.bases,
            q30_bases: self.q30,
            gc_bases: self.gc,
            n_bases: self.n,
            mean_len: self.bases as f64 / self.reads.max(1) as f64,
            q30_pct: self.q30 as f64 / self.bases.max(1) as f64 * 100.0,
            gc_pct: self.gc as f64 / self.bases.max(1) as f64 * 100.0,
            n_pct: self.n as f64 / self.bases.max(1) as f64 * 100.0,
            panel_coverage: called as f64 / total.max(1) as f64 * 100.0,
            length_histogram: self.lengths.iter().map(|(a, b)| (*a, *b)).collect(),
        }
    }
}
