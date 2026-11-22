use crate::{KMER_SIZE, Panel, Variant, allele_kmers};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hit {
    pub variant: u16,
    pub allele: u8,
    pub offset: u8,
    pub strand: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadHit {
    pub variant: u16,
    pub allele: u8,
    pub offset: u8,
    pub strand: u8,
    pub start: u32,
}

pub fn hash(key: u64) -> u64 {
    key.wrapping_mul(0x9E3779B97F4A7C15) >> 43
}
pub const EMPTY: Hit = Hit {
    variant: u16::MAX,
    allele: 0,
    offset: 0,
    strand: 0,
};

#[derive(Debug)]
pub struct PanelIndex {
    pub variants: Arc<Vec<Variant>>,
    pub bitmap: Vec<u64>,
    pub slots: Vec<(u64, Vec<Hit>)>,
    pub mask: usize,
    pub empty_count: usize,
    pub simd: crate::encode::SimdLevel,
}
impl PanelIndex {
    /// Construct the reference once per scan. SIMD is selected from runtime
    /// CPUID features; explicitly unsupported levels return an error upstream.
    pub fn with_simd(panel: &Panel, level: crate::encode::SimdLevel) -> Self {
        let mut index = Self::new(panel);
        index.simd = level;
        index
    }
    pub fn new(panel: &Panel) -> Self {
        let candidates: usize = panel
            .variants
            .iter()
            .map(|v| (v.alts.len() + 1) * KMER_SIZE * 2)
            .sum();
        let capacity = (candidates.max(1) * 2).next_power_of_two();
        let mut this = Self {
            variants: Arc::new(panel.variants.clone()),
            bitmap: vec![0; 2_097_152 / 64],
            slots: vec![(0, Vec::new()); capacity],
            mask: capacity - 1,
            empty_count: 0,
            simd: crate::encode::SimdLevel::Scalar,
        };
        let mut keys: HashMap<u64, Vec<Hit>> = HashMap::new();
        let mut owners: HashMap<u64, HashSet<(u16, u8)>> = HashMap::new();
        for (id, variant) in panel.variants.iter().enumerate().take(u16::MAX as usize) {
            for allele in 0..variant.alts.len() + 1 {
                for (key, _hit) in allele_kmers(variant, allele) {
                    owners
                        .entry(key)
                        .or_default()
                        .insert((id as u16, allele as u8));
                }
            }
        }
        for (id, variant) in panel.variants.iter().enumerate().take(u16::MAX as usize) {
            for allele in 0..variant.alts.len() + 1 {
                for (key, mut hit) in allele_kmers(variant, allele) {
                    hit.variant = id as u16;
                    if owners.get(&key).is_some_and(|set| set.len() == 1) {
                        keys.entry(key).or_default().push(hit);
                    }
                }
            }
        }
        for (key, hit) in keys {
            this.insert(key, hit);
        }
        this.empty_count = this.slots.iter().filter(|(k, _)| *k == 0).count();
        this
    }
    pub fn insert(&mut self, key: u64, hits: Vec<Hit>) {
        let h = hash(key);
        self.bitmap[(h >> 6) as usize] |= 1 << (h & 63);
        let mut slot = (h as usize) & self.mask;
        while self.slots[slot].0 != 0 {
            if self.slots[slot].0 == key + 1 {
                self.slots[slot].1.extend(hits);
                return;
            }
            slot = (slot + 1) & self.mask;
        }
        self.slots[slot] = (key + 1, hits);
    }
    /// Copy-free in-place lookup avoids allocating a vector per k-mer.
    pub fn lookup_ref(&self, key: u64) -> Option<&[Hit]> {
        let h = hash(key);
        if self.bitmap[(h >> 6) as usize] & (1 << (h & 63)) == 0 {
            return None;
        }
        let mut slot = (h as usize) & self.mask;
        for _ in 0..=self.mask {
            let (stored, ref hit) = self.slots[slot];
            if stored == key + 1 {
                return Some(hit);
            }
            if stored == 0 {
                return None;
            }
            slot = (slot + 1) & self.mask;
        }
        None
    }
    pub fn raw_lookup(&self, key: u64) -> Option<Vec<Hit>> {
        let mut slot = (hash(key) as usize) & self.mask;
        for _ in 0..=self.mask {
            let (stored, ref hit) = self.slots[slot];
            if stored == key + 1 {
                return Some(hit.clone());
            }
            if stored == 0 {
                return None;
            }
            slot = (slot + 1) & self.mask;
        }
        None
    }
    pub fn lookup(&self, key: u64) -> Option<Vec<Hit>> {
        let h = hash(key);
        if self.bitmap[(h >> 6) as usize] & (1 << (h & 63)) == 0 {
            return None;
        }
        self.raw_lookup(key)
    }
    pub fn read_hits(&self, seq: &[u8]) -> Vec<ReadHit> {
        let mut encoder = crate::encode::KmerEncoder::new(self.simd);
        let mut hits = Vec::new();
        self.read_hits_with(seq, &mut encoder, &mut hits);
        hits
    }
    /// Reuse the packed-base buffer and result across the reads in a worker.
    pub fn read_hits_with(
        &self,
        seq: &[u8],
        encoder: &mut crate::encode::KmerEncoder,
        hits: &mut Vec<ReadHit>,
    ) {
        hits.clear();
        if seq.len() < KMER_SIZE {
            return;
        }
        let mut keys = Vec::new();
        encoder.kmer_keys_into(seq, &mut keys);
        for key in keys {
            if let Some(table_hits) = self.lookup_ref(key.canonical) {
                for &hit in table_hits
                    .iter()
                    .filter(|hit| (hit.strand == 0) == key.forward)
                {
                    hits.push(ReadHit {
                        variant: hit.variant,
                        allele: hit.allele,
                        offset: hit.offset,
                        strand: hit.strand,
                        start: key.start as u32,
                    });
                }
            }
        }
    }
    /// A read contributes at most one vote per variant. Conflicting alleles
    /// supported by distinct event offsets invalidate that read, not just a k-mer.
    pub fn vote(&self, seq: &[u8], qual: &[u8]) -> Option<(usize, u8)> {
        self.vote_hits(seq, qual, &self.read_hits(seq))
    }
    pub fn vote_hits(&self, seq: &[u8], qual: &[u8], hits: &[ReadHit]) -> Option<(usize, u8)> {
        self.vote_hits_filtered(seq, qual, None, hits)
    }
    /// Vote one already identified variant without copying its hits into a second vector.
    pub fn vote_variant_hits(
        &self,
        seq: &[u8],
        qual: &[u8],
        variant: usize,
        hits: &[ReadHit],
    ) -> Option<u8> {
        self.vote_hits_filtered(seq, qual, Some(variant), hits)
            .map(|(called, allele)| {
                debug_assert_eq!(called, variant);
                allele
            })
    }
    fn vote_hits_filtered(
        &self,
        seq: &[u8],
        qual: &[u8],
        selected: Option<usize>,
        hits: &[ReadHit],
    ) -> Option<(usize, u8)> {
        let mut per_variant: PerVariant = HashMap::new();
        for h in hits {
            let variant = h.variant as usize;
            if selected.is_some_and(|selected| selected != variant) {
                continue;
            }
            let Some(v) = self.variants.get(variant) else {
                continue;
            };
            let allele = if h.allele == 0 {
                &v.ref_allele
            } else {
                &v.alts[h.allele as usize - 1]
            };
            // An empty allele checks its preceding base. In reverse reads
            // that is immediately after (not within) the junction k-mer.
            let position = if allele.is_empty() {
                let junction = if h.strand == 0 {
                    h.start as usize + h.offset as usize + 1
                } else {
                    seq.len() - h.start as usize - KMER_SIZE
                };
                junction.saturating_sub(1)
            } else if h.strand == 0 {
                h.start as usize + h.offset as usize
            } else {
                seq.len() - h.start as usize - 1 - h.offset as usize
            };
            if allele.is_empty()
                && ((h.strand == 0 && h.offset as usize == KMER_SIZE - 1)
                    || (h.strand == 1 && h.offset == 0))
            {
                continue;
            }
            let (Some(&q), Some(&base)) = (qual.get(position), seq.get(position)) else {
                continue;
            };
            if q < 20 || crate::base_encode(base).is_none() {
                continue;
            }
            // Both stored orientations map to the same physical event. Keep
            // the valid observed coordinate, never a synthetic alias.
            let location = position;
            let entry = per_variant
                .entry(variant)
                .or_default()
                .entry(location)
                .or_insert((0, 0));
            entry.0 += 1;
            entry.1 |= 1 << h.allele;
        }
        let mut result = Vec::new();
        for (variant, locations) in per_variant {
            let mut counts = HashMap::new();
            for (_, (_, alleles)) in locations {
                let v = &self.variants[variant];
                for allele in 0..=v.alts.len() {
                    if alleles & (1 << allele) != 0 {
                        *counts.entry(allele as u8).or_insert(0usize) += 1;
                    }
                }
            }
            let supported: Vec<_> = counts.iter().filter(|(_, n)| **n >= 2).collect();
            if supported.len() == 1 && counts.values().filter(|n| **n > 0).count() == 1 {
                result.push((variant, *supported[0].0));
            }
        }
        if result.len() == 1 {
            Some(result[0])
        } else if result.is_empty() {
            None
        } else {
            panic!("read matched multiple panel variants")
        }
    }
}

/// Per-variant event locations: count and bitmask of supporting alleles.
type PerVariant = HashMap<usize, HashMap<usize, (usize, u8)>>;

pub fn scan_read(index: &PanelIndex, seq: &[u8], qual: &[u8], counts: &mut [[u32; 4]]) {
    let mut encoder = crate::encode::KmerEncoder::new(index.simd);
    let mut hits = Vec::new();
    scan_read_with(index, seq, qual, counts, &mut encoder, &mut hits);
}

pub fn scan_read_with(
    index: &PanelIndex,
    seq: &[u8],
    qual: &[u8],
    counts: &mut [[u32; 4]],
    encoder: &mut crate::encode::KmerEncoder,
    hits: &mut Vec<ReadHit>,
) {
    index.read_hits_with(seq, encoder, hits);
    let mut variants = hits
        .iter()
        .map(|h| h.variant as usize)
        .collect::<HashSet<_>>();
    for v in variants.drain() {
        if let Some(allele) = index.vote_variant_hits(seq, qual, v, hits) {
            counts[v][allele.min(3) as usize] += 1;
        }
    }
}

/// Scan panel and sample markers with a single encoded k-mer traversal.
/// Marker evidence stays boolean per read, never a count of matching k-mers.
pub fn scan_read_with_markers(
    index: &PanelIndex,
    read: (&[u8], &[u8]),
    counts: &mut [[u32; 4]],
    encoder: &mut crate::encode::KmerEncoder,
    hits: &mut Vec<ReadHit>,
    scanner: &mut crate::marker::MarkerScanner,
    local: &mut crate::marker::MarkerLocal,
) {
    let (seq, qual) = read;
    hits.clear();
    let mut y = false;
    let mut control = false;
    encoder.for_each_key(seq, |key| {
        crate::marker::observe_marker_key(&scanner.index, key, &mut y, &mut control);
        if let Some(table_hits) = index.lookup_ref(key.canonical) {
            for &hit in table_hits
                .iter()
                .filter(|hit| (hit.strand == 0) == key.forward)
            {
                hits.push(ReadHit {
                    variant: hit.variant,
                    allele: hit.allele,
                    offset: hit.offset,
                    strand: hit.strand,
                    start: key.start as u32,
                });
            }
        }
    });
    local.y += u64::from(y);
    local.control += u64::from(control);
    let mut variants = hits
        .iter()
        .map(|h| h.variant as usize)
        .collect::<HashSet<_>>();
    for v in variants.drain() {
        if let Some(allele) = index.vote_variant_hits(seq, qual, v, hits) {
            counts[v][allele.min(3) as usize] += 1;
        }
    }
}
