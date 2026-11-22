use crate::{
    Panel, PanelIndex,
    fastq::{FastqReader, gzip_reader},
    genotype::{GenotypeCall, call},
    index::scan_read,
    marker::{MarkerIndex, MarkerLocal, sample_checks},
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
}
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub calls: Vec<GenotypeCall>,
    pub qc: crate::qc::Qc,
    pub checks: crate::marker::SampleChecks,
    pub elapsed_seconds: f64,
    pub engine: String,
}
pub fn scan_path(
    path: &Path,
    panel: &Panel,
    threads: usize,
    progress: Option<&Progress>,
) -> Result<ScanResult> {
    let start = Instant::now();
    let index = Arc::new(PanelIndex::new(panel));
    let markers_index = Arc::new(MarkerIndex::new(panel));
    let file = std::fs::File::open(path)?;
    let source: Box<dyn std::io::Read + Send> = if path.extension().is_some_and(|x| x == "gz") {
        gzip_reader(path)?
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
                while let Ok(mut batch) = rx.recv() {
                    if let Some(p) = progress {
                        p.bytes.fetch_add(batch.bytes, Ordering::Relaxed);
                        p.reads
                            .fetch_add(batch.records.len() as u64, Ordering::Relaxed);
                    }
                    for r in &batch.records {
                        let seq = &batch.buf[r.seq_off..r.seq_off + r.seq_len];
                        let qual = &batch.buf[r.qual_off..r.qual_off + r.seq_len];
                        scan_read(&index, seq, qual, &mut c);
                        q.add(seq, qual);
                        m.add(seq, &markers_index);
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
    Ok(ScanResult {
        calls,
        qc,
        checks,
        elapsed_seconds: start.elapsed().as_secs_f64(),
        engine: format!("CPU scalar; {} worker threads", workers),
    })
}
