use crate::{
    Panel, PanelIndex,
    fastq::{FastqReader, gzip_reader},
    genotype::{GenotypeCall, call},
    marker::{MarkerIndex, MarkerLocal, MarkerScanner, sample_checks},
    qc::QcLocal,
};
use anyhow::Result;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub bytes: Arc<AtomicU64>,
    pub reads: Arc<AtomicU64>,
    pub bases: Arc<AtomicU64>,
    pub votes: Arc<Vec<[std::sync::atomic::AtomicU32; 4]>>,
}
impl Progress {
    pub fn new(variants: usize) -> Self {
        Self {
            votes: Arc::new(
                (0..variants)
                    .map(|_| std::array::from_fn(|_| 0.into()))
                    .collect(),
            ),
            ..Self::default()
        }
    }
    pub fn update_votes(&self, delta: &[[u32; 4]]) {
        for (live, delta) in self.votes.iter().zip(delta) {
            for (live, delta) in live.iter().zip(delta) {
                live.fetch_add(*delta, Ordering::Relaxed);
            }
        }
    }
    pub fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.reads.store(0, Ordering::Relaxed);
        self.bases.store(0, Ordering::Relaxed);
        for counts in self.votes.iter() {
            for count in counts {
                count.store(0, Ordering::Relaxed);
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub calls: Vec<GenotypeCall>,
    pub qc: crate::qc::Qc,
    pub checks: crate::marker::SampleChecks,
    pub elapsed_seconds: f64,
    pub engine: String,
}
/// Prebuilt immutable indexes shared by scans of the same panel.
pub struct ScanContext {
    pub index: Arc<PanelIndex>,
    pub markers: Arc<MarkerIndex>,
}
impl ScanContext {
    pub fn new(panel: &Panel, level: crate::encode::SimdLevel) -> Self {
        Self {
            index: Arc::new(PanelIndex::with_simd(panel, level)),
            markers: Arc::new(MarkerIndex::new(panel)),
        }
    }
}
pub fn scan_path(
    path: &Path,
    panel: &Panel,
    threads: usize,
    progress: Option<&Progress>,
) -> Result<ScanResult> {
    scan_path_with_simd(path, panel, threads, progress, crate::encode::runtime())
}
pub fn scan_path_with_simd(
    path: &Path,
    panel: &Panel,
    threads: usize,
    progress: Option<&Progress>,
    level: crate::encode::SimdLevel,
) -> Result<ScanResult> {
    if !level.available() {
        anyhow::bail!("requested SIMD level {level:?} is not supported by this CPU");
    }
    scan_path_with_context(path, panel, threads, progress, level, None)
}
pub fn scan_path_with_context(
    path: &Path,
    panel: &Panel,
    threads: usize,
    progress: Option<&Progress>,
    level: crate::encode::SimdLevel,
    context: Option<&ScanContext>,
) -> Result<ScanResult> {
    let start = Instant::now();
    let index = context
        .map(|context| context.index.clone())
        .unwrap_or_else(|| Arc::new(PanelIndex::with_simd(panel, level)));
    let markers_index = context
        .map(|context| context.markers.clone())
        .unwrap_or_else(|| Arc::new(MarkerIndex::new(panel)));
    let file = std::fs::File::open(path)?;
    let source: Box<dyn std::io::Read + Send> = if path.extension().is_some_and(|x| x == "gz") {
        gzip_reader(file)?
    } else {
        Box::new(file)
    };
    let (recycle_tx, recycle_rx) = crossbeam_channel::bounded(2 * threads.max(1) + 2);
    let mut reader = FastqReader::with_recycler(source, 8 * 1024 * 1024, recycle_rx);
    let mut counts = vec![[0u32; 4]; panel.variants.len()];
    let mut qc = QcLocal::default();
    let mut markers = MarkerLocal::default();
    let workers = threads.max(1);
    let (local_tx, local_rx) =
        crossbeam_channel::unbounded::<(Vec<[u32; 4]>, QcLocal, MarkerLocal)>();
    let (tx, rx) = crossbeam_channel::bounded::<crate::fastq::Batch>((workers * 2).max(1));
    std::thread::scope(|s| {
        for _ in 0..workers {
            let rx = rx.clone();
            let index = index.clone();
            let markers_index = markers_index.clone();
            let local_tx = local_tx.clone();
            let recycle_tx = recycle_tx.clone();
            s.spawn(move || {
                let mut c = vec![[0u32; 4]; index.variants.len()];
                let mut q = QcLocal::default();
                let mut m = MarkerLocal::default();
                let mut marker_scanner = MarkerScanner::new(markers_index, level);
                let mut encoder = crate::encode::KmerEncoder::new(level);
                let mut hits = Vec::new();
                let mut previous = vec![[0u32; 4]; index.variants.len()];
                let mut delta = vec![[0u32; 4]; index.variants.len()];
                while let Ok(mut batch) = rx.recv() {
                    if let Some(p) = progress {
                        p.bytes.fetch_add(batch.bytes, Ordering::Relaxed);
                        p.reads
                            .fetch_add(batch.records.len() as u64, Ordering::Relaxed);
                    }
                    for r in &batch.records {
                        let seq = &batch.buf[r.seq_off..r.seq_off + r.seq_len];
                        let qual = &batch.buf[r.qual_off..r.qual_off + r.seq_len];
                        crate::index::scan_read_with_markers(
                            &index,
                            (seq, qual),
                            &mut c,
                            &mut encoder,
                            &mut hits,
                            &mut marker_scanner,
                            &mut m,
                        );
                        marker_scanner.add_telomere(seq, &mut m);
                        q.add(seq, qual);
                        if let Some(p) = progress {
                            p.bases.fetch_add(r.seq_len as u64, Ordering::Relaxed);
                        }
                    }
                    if let Some(p) = progress {
                        for ((now, old), delta) in c.iter().zip(&previous).zip(&mut delta) {
                            for allele in 0..4 {
                                delta[allele] = now[allele] - old[allele];
                            }
                        }
                        p.update_votes(&delta);
                        previous.copy_from_slice(&c);
                    }
                    batch.records.clear();
                    let _ = recycle_tx.try_send(std::mem::take(&mut batch.buf));
                }
                let _ = local_tx.send((c, q, m));
            });
        }
        drop(rx);
        drop(local_tx);
        drop(recycle_tx);
        let dispatch = s.spawn(move || -> Result<()> {
            while let Some(batch) = reader.next_batch()? {
                tx.send(batch)?;
            }
            drop(tx);
            Ok(())
        });
        for (c, q, m) in local_rx {
            for (sum, value) in counts.iter_mut().flatten().zip(c.into_iter().flatten()) {
                *sum += value;
            }
            qc.merge(&q);
            markers.merge(&m);
        }
        dispatch
            .join()
            .map_err(|_| anyhow::anyhow!("FASTQ reader panicked"))?
    })?;
    Ok(finish(
        panel,
        counts,
        qc,
        markers,
        start.elapsed().as_secs_f64(),
        format!("CPU {level:?}; {} worker threads", workers),
    ))
}
/// Shared genotype and sample-check finalization for CPU and GPU backends.
pub fn finish(
    panel: &Panel,
    counts: Vec<[u32; 4]>,
    qc: QcLocal,
    markers: MarkerLocal,
    elapsed_seconds: f64,
    engine: String,
) -> ScanResult {
    let calls = panel
        .variants
        .iter()
        .zip(&counts)
        .map(|(v, c)| call(v, c, "unclear"))
        .collect::<Vec<_>>();
    let called = calls.iter().filter(|c| c.called).count();
    let preliminary_qc = qc.finish(called, panel.variants.len());
    let checks = sample_checks(panel, &calls, &preliminary_qc, &markers);
    let sex = checks.sex.classification.clone();
    let calls = panel
        .variants
        .iter()
        .zip(&counts)
        .map(|(v, c)| call(v, c, &sex))
        .collect::<Vec<_>>();
    let called = calls.iter().filter(|c| c.called).count();
    let qc = qc.finish(called, panel.variants.len());
    let checks = sample_checks(panel, &calls, &qc, &markers);
    ScanResult {
        calls,
        qc,
        checks,
        elapsed_seconds,
        engine,
    }
}
