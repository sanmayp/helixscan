pub mod encode;
pub mod fastq;
pub mod genotype;
pub mod index;
pub mod marker;
pub mod pgx;
pub mod qc;
pub mod report;
pub mod scan;
pub use helixscan_panel::{Panel, Variant};
pub use index::PanelIndex;

pub const KMER_SIZE: usize = 31;
pub const KMER_MASK: u64 = (1u64 << 62) - 1;

pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|b| match b.to_ascii_uppercase() {
            b'A' => b'T',
            b'C' => b'G',
            b'G' => b'C',
            b'T' => b'A',
            _ => b'N',
        })
        .collect()
}

/// Build a complete allele context. allele_offset locates the normalized event
/// relative to the end of the left flank; REF itself is never deleted.
pub use helixscan_panel::allele_context;

pub fn kmer(sequence: &[u8]) -> Option<u64> {
    if sequence.len() != KMER_SIZE {
        return None;
    }
    let mut f = 0u64;
    let mut r = 0u64;
    for b in sequence {
        let a = match b | 32 {
            b'a' => 0,
            b'c' => 1,
            b'g' => 2,
            b't' => 3,
            _ => return None,
        };
        f = ((f << 2) | a) & KMER_MASK;
        r = (r >> 2) | ((3 - a) << 60);
    }
    Some(f.min(r))
}

pub fn base_encode(base: u8) -> Option<u64> {
    match base | 32 {
        b'a' => Some(0),
        b'c' => Some(1),
        b'g' => Some(2),
        b't' => Some(3),
        _ => None,
    }
}

pub fn canonical(seq: &[u8]) -> Option<u64> {
    kmer(seq)
}

/// All unambiguous 31-mers crossing an allele, in both read orientations.
/// `offset` is a real event base; a deletion checks the preceding base.
pub fn allele_kmers(v: &Variant, allele_index: usize) -> Vec<(u64, crate::index::Hit)> {
    let allele = if allele_index == 0 {
        &v.ref_allele
    } else {
        &v.alts[allele_index - 1]
    };
    let (seq, pos) = allele_context(v, allele);
    let mut result = Vec::new();
    if pos == 0 || seq.len() < KMER_SIZE {
        return result;
    }
    let event_base = if allele.is_empty() { pos - 1 } else { pos };
    // In the mutant coordinate system this is either the altered base(s),
    // or the empty span at the deletion junction.
    let event_end = pos + allele.len();
    let (first_start, maximum_start) = if allele.is_empty() {
        (pos.saturating_sub(KMER_SIZE - 1), pos.saturating_sub(1))
    } else {
        (
            (pos + 1).saturating_sub(KMER_SIZE),
            event_end.saturating_sub(1),
        )
    };
    let maximum_start = maximum_start.min(seq.len() - KMER_SIZE);
    if first_start > maximum_start {
        return result;
    }
    for start in first_start..=maximum_start {
        if allele.is_empty() && (start >= pos || start + KMER_SIZE <= pos) {
            continue;
        }
        let offset = event_base.saturating_sub(start).min(30);
        let forward = &seq[start..start + KMER_SIZE];
        let reverse = reverse_complement(forward);
        for strand in 0..=1u8 {
            let bases = if strand == 0 {
                forward.to_vec()
            } else {
                reverse.clone()
            };
            if let Some(key) = kmer(&bases) {
                result.push((
                    key,
                    crate::index::Hit {
                        variant: 0,
                        allele: allele_index as u8,
                        offset: offset as u8,
                        strand,
                    },
                ));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
