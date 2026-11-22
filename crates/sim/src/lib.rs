use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;

pub fn reverse_complement(seq: &mut [u8]) {
    seq.reverse();
    for base in seq {
        *base = match *base {
            b'A' => b'T',
            b'C' => b'G',
            b'G' => b'C',
            b'T' => b'A',
            x => x,
        };
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Truth {
    pub rsid: String,
    pub genotype: String,
    pub dosage: u8,
    pub copies: Vec<String>,
    pub chromosome: String,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum Sex {
    #[default]
    Auto,
    Xx,
    Xy,
}
pub fn chunk_reads(
    panel: &helixscan_panel::Panel,
    id_start: u64,
    count: usize,
    config: &Config,
    seeded: &[Truth],
) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(config.seed.wrapping_add(id_start));
    let mut out = Vec::with_capacity(count * (config.read_len * 2 + 60));
    let sequence_len = config.read_len;
    let variant_total = panel.variants.len() * config.depth as usize;
    for i in 0..count {
        let id = id_start + i as u64;
        let mut seq = Vec::new();
        let mut qual = Vec::new();
        if (id as usize) < variant_total {
            let v = &panel.variants[(id as usize) / config.depth as usize];
            let truth = &seeded[(id as usize) / config.depth as usize];
            let allele = &truth.copies[(id as usize % config.depth as usize) % 2];
            let (context, context_pos) = helixscan_panel::allele_context(v, allele);
            // The panel stores 60 bases on each side. Extend that verified
            // context with background sequence for reads longer than 121 bp.
            let left_pad = sequence_len.saturating_sub(context_pos);
            let right_pad = (sequence_len - 31).saturating_sub(context.len() - context_pos);
            let mut full = vec![0; left_pad];
            fill_background(&mut full, &mut rng);
            full.extend_from_slice(&context);
            let old_len = full.len();
            full.resize(old_len + right_pad, 0);
            fill_background(&mut full[old_len..], &mut rng);
            let variant_pos = context_pos + left_pad;
            let lower = variant_pos.saturating_sub(sequence_len - 31);
            let upper = variant_pos
                .saturating_sub(30)
                .min(full.len().saturating_sub(sequence_len));
            let start = rng.random_range(lower..=upper);
            seq = full[start..start + sequence_len].to_vec();
            if rng.random_bool(0.5) {
                reverse_complement(&mut seq);
            }
            for (i, base) in seq.iter_mut().enumerate() {
                let error = 0.0003 + 0.002 * i as f64 / (sequence_len.max(2) - 1) as f64;
                if rng.random::<f64>() < 0.005 {
                    *base = b'N';
                } else if rng.random::<f64>() < error {
                    *base = match *base {
                        b'A' => b'C',
                        b'C' => b'G',
                        b'G' => b'T',
                        b'T' => b'A',
                        x => x,
                    };
                }
                qual.push(if rng.random::<f64>() < 0.06 {
                    rng.random_range(7..20)
                } else {
                    rng.random_range(25..41)
                });
            }
            out.extend_from_slice(format!("@sim-{id}-{}\n", v.rsid).as_bytes());
        } else {
            let record = make_record(&mut rng, id, &mut seq, &mut qual, sequence_len);
            out.extend_from_slice(record.as_bytes());
            continue;
        }
        out.extend_from_slice(&seq);
        out.extend_from_slice(b"\n+\n");
        out.extend(qual.iter().map(|q| q + 33));
        out.push(b'\n');
    }
    out
}

pub fn truths(panel: &helixscan_panel::Panel, config: &Config) -> Vec<Truth> {
    let mut rng = SmallRng::seed_from_u64(config.seed);
    panel
        .variants
        .iter()
        .map(|v| {
            let (copies, genotype, dosage) = genotype(&mut rng, v, config.population.as_deref());
            Truth {
                rsid: v.rsid.clone(),
                genotype,
                dosage,
                copies,
                chromosome: v.chr.clone(),
            }
        })
        .collect()
}

pub struct Config {
    pub seed: u64,
    pub depth: u32,
    pub read_len: usize,
    pub population: Option<String>,
    pub sex: Sex,
    pub telomeric_fraction: f64,
}
pub fn genotype(
    rng: &mut SmallRng,
    variant: &helixscan_panel::Variant,
    population: Option<&str>,
) -> (Vec<String>, String, u8) {
    let all = std::iter::once(&variant.ref_allele)
        .chain(variant.alts.iter())
        .collect::<Vec<_>>();
    let (a, b) = if let Some(pop) = population {
        let freq = pop_frequency(variant, pop);
        let pick = |rng: &mut SmallRng| -> usize {
            let x: f64 = rng.random();
            let mut sum = 0.0;
            for (i, p) in freq.iter().enumerate() {
                sum += p;
                if x <= sum {
                    return i;
                }
            }
            0
        };
        (pick(rng), pick(rng))
    } else {
        let n = rng.random_range(0..3);
        if n == 0 {
            (0, 0)
        } else if n == 1 {
            (0, rng.random_range(1..all.len()))
        } else {
            let a = rng.random_range(1..all.len());
            (a, a)
        }
    };
    if a == b {
        (
            vec![all[a].clone(), all[b].clone()],
            format!("{}/{}", all[a], all[a]),
            if a == 0 { 0 } else { 2 },
        )
    } else {
        (
            vec![all[a].clone(), all[b].clone()],
            format!("{}/{}", all[a], all[b]),
            1,
        )
    }
}
fn pop_frequency(v: &helixscan_panel::Variant, pop: &str) -> Vec<f64> {
    let f = v.frequencies.get(pop);
    let mut p = vec![0.0];
    p.resize(v.alts.len() + 1, 0.0);
    for (i, a) in v.alts.iter().enumerate() {
        p[i + 1] = f
            .and_then(|m| m.get(a))
            .copied()
            .unwrap_or(0.01)
            .clamp(0.01, 0.98);
    }
    p[0] = (1.0 - p.iter().skip(1).sum::<f64>()).max(0.01);
    let z: f64 = p.iter().sum();
    p.into_iter().map(|x| x / z).collect()
}
pub fn fill_background(seq: &mut [u8], rng: &mut SmallRng) {
    for x in seq {
        *x = if rng.random::<f64>() < 0.41 {
            if rng.random_bool(0.5) { b'C' } else { b'G' }
        } else if rng.random_bool(0.5) {
            b'A'
        } else {
            b'T'
        }
    }
}
pub fn make_record(
    rng: &mut SmallRng,
    id: u64,
    seq: &mut Vec<u8>,
    quality: &mut Vec<u8>,
    len: usize,
) -> String {
    seq.resize(len, 0);
    quality.resize(len, 0);
    fill_background(seq, rng);
    if rng.random_bool(0.5) {
        reverse_complement(seq);
    }
    for (i, q) in quality.iter_mut().enumerate() {
        let error = 0.0003 + 0.002 * i as f64 / (len.max(2) - 1) as f64;
        let n = rng.random::<f64>() < 0.01;
        if n {
            seq[i] = b'N';
        }
        if rng.random::<f64>() < error {
            seq[i] = match seq[i] {
                b'A' => b'C',
                b'C' => b'G',
                b'G' => b'T',
                b'T' => b'A',
                x => x,
            };
        }
        *q = if n || rng.random::<f64>() < 0.06 {
            rng.random_range(7..20)
        } else {
            rng.random_range(25..41)
        };
    }
    format!(
        "@sim-{id}\n{}\n+\n{}\n",
        String::from_utf8_lossy(seq),
        quality
            .iter()
            .map(|q| char::from(q + 33))
            .collect::<String>()
    )
}
pub fn parallel_fastq<W: std::io::Write + Send>(
    mut out: W,
    panel: &helixscan_panel::Panel,
    reads: u64,
    config: &Config,
    truth: &[Truth],
) -> std::io::Result<()> {
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::sync::mpsc::sync_channel;
    if reads == 0 {
        return Ok(());
    }
    let chunk_size = 8192usize;
    let chunks = reads.div_ceil(chunk_size as u64) as usize;
    let (tx, rx) =
        sync_channel::<(usize, std::io::Result<Vec<u8>>)>(rayon::current_num_threads() * 2);
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || -> std::io::Result<()> {
            let mut next = 0;
            let mut waiting = BTreeMap::new();
            for (id, result) in rx {
                waiting.insert(id, result);
                while let Some(result) = waiting.remove(&next) {
                    out.write_all(&result?)?;
                    next += 1;
                }
            }
            Ok(())
        });
        (0..chunks).into_par_iter().for_each_with(tx, |tx, chunk| {
            let start = chunk * chunk_size;
            let count = chunk_size.min(reads as usize - start);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                chunk_reads(panel, start as u64, count, config, truth)
            }))
            .map_err(|_| std::io::Error::other("simulator worker panicked"));
            let _ = tx.send((chunk, result));
        });
        writer
            .join()
            .map_err(|_| std::io::Error::other("simulator writer panicked"))?
    })
}
