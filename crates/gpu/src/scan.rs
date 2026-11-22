//! Portable GPU matching with a verified, explicit CPU fallback.
//! Only candidate enumeration runs on WebGPU. Quality voting, genotype calls,
//! QC, marker checks and report inputs remain the authoritative CPU paths.
use anyhow::Result;
use helixscan_core::{
    Panel, PanelIndex,
    encode::SimdLevel,
    fastq::FastqReader,
    qc::QcLocal,
    scan::{Progress, ScanResult},
};
use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

/// The P5000's measured 45.691 s GPU transfer/matching/readback time versus
/// 1.320 s of complete CPU work on demo-1gb.fastq (11 workers) is too large
/// for calibration to repay. Forced GPU mode still verifies real GPU results.
const AUTO_CPU_POLICY: &str = "auto: CPU; Quadro P5000 measured 45.691s vs CPU 1.320s on demo-1gb.fastq (11 workers); CPU faster because GPU batch transfers + readback cost dominates; no GPU trial; use --backend gpu to benchmark";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
#[derive(Clone, Debug)]
pub struct Selection {
    pub result: ScanResult,
    pub gpu_device: Option<String>,
    pub gpu_stats: Option<crate::GpuStats>,
    pub fallback_reason: Option<String>,
}
impl Selection {
    pub fn cpu(result: ScanResult) -> Self {
        Self {
            result,
            gpu_device: None,
            gpu_stats: None,
            fallback_reason: None,
        }
    }
    fn fallback(
        result: ScanResult,
        device: Option<String>,
        stats: Option<crate::GpuStats>,
        reason: String,
    ) -> Self {
        Self {
            result,
            gpu_device: device,
            gpu_stats: stats,
            fallback_reason: Some(reason),
        }
    }
}

pub fn scan_path(
    path: &Path,
    panel: &Panel,
    threads: usize,
    progress: Option<&Progress>,
    backend: Backend,
    config: crate::GpuConfig,
) -> Result<Selection> {
    let simd = helixscan_core::encode::runtime();
    if matches!(backend, Backend::Cpu | Backend::Auto) {
        let mut result =
            helixscan_core::scan::scan_path_with_simd(path, panel, threads, progress, simd)?;
        if backend == Backend::Auto {
            result.engine = format!("{}; {AUTO_CPU_POLICY}", result.engine);
        }
        return Ok(Selection::cpu(result));
    }
    let cpu = helixscan_core::scan::scan_path_with_simd(path, panel, threads, progress, simd)?;
    let cpu_elapsed = cpu.elapsed_seconds;
    let index = PanelIndex::with_simd(panel, simd);
    let mut matcher = match crate::GpuMatcher::new(&index, config) {
        Ok(matcher) => matcher,
        Err(error) => {
            if backend == Backend::Gpu {
                return Err(error);
            }
            return Ok(Selection::fallback(
                cpu,
                None,
                None,
                format!("GPU unavailable: {error}; used CPU"),
            ));
        }
    };
    if let Err(error) = matcher.calibrate(&index) {
        if backend == Backend::Gpu {
            return Err(error);
        }
        return Ok(Selection::fallback(
            cpu,
            Some(matcher.stats().device.clone()),
            None,
            format!("GPU calibration failed: {error}; used CPU"),
        ));
    }
    let (mut candidate, stats) =
        match gpu_scan_with_index(path, panel, simd, &mut matcher, threads, progress, index) {
            Ok(x) => x,
            Err(error) => {
                if backend == Backend::Gpu {
                    return Err(error);
                }
                return Ok(Selection::fallback(
                    cpu,
                    Some(matcher.stats().device.clone()),
                    None,
                    format!("GPU candidate failed: {error}; used CPU"),
                ));
            }
        };
    let same = candidate.calls == cpu.calls
        && serde_json::to_vec(&candidate.qc)? == serde_json::to_vec(&cpu.qc)?
        && serde_json::to_vec(&candidate.checks)? == serde_json::to_vec(&cpu.checks)?;
    if !same {
        if backend == Backend::Gpu {
            anyhow::bail!("GPU/CPU full-result mismatch (calls, QC, or sample checks)");
        }
        return Ok(Selection::fallback(
            cpu,
            Some(matcher.stats().device.clone()),
            Some(stats),
            "GPU full-result parity failed; used verified CPU result".into(),
        ));
    }
    if backend == Backend::Auto && candidate.elapsed_seconds >= cpu_elapsed {
        return Ok(Selection::fallback(
            cpu,
            Some(stats.device.clone()),
            Some(stats.clone()),
            format!(
                "GPU matched the complete CPU result but took {:.3}s vs CPU {:.3}s; used CPU ({} workgroup {}, {} MiB cap)",
                candidate.elapsed_seconds,
                cpu_elapsed,
                stats.device,
                stats.workgroup,
                stats.memory_cap_mb
            ),
        ));
    }
    candidate.engine = format!(
        "GPU {}; driver {}; core Vulkan WGSL; workgroup {}; card memory {} MiB; cap {} MiB; CPU votes/QC",
        stats.device,
        stats.driver,
        stats.workgroup,
        stats
            .memory_mb
            .map_or_else(|| "unknown".to_owned(), |x| x.to_string()),
        stats.memory_cap_mb
    );
    Ok(Selection {
        result: candidate,
        gpu_device: Some(stats.device.clone()),
        gpu_stats: Some(stats),
        fallback_reason: None,
    })
}

/// The scan owns one table, also used by matcher initialization/calibration.
pub fn gpu_scan_with_index(
    path: &Path,
    panel: &Panel,
    simd: SimdLevel,
    matcher: &mut crate::GpuMatcher,
    threads: usize,
    progress: Option<&Progress>,
    index: PanelIndex,
) -> Result<(ScanResult, crate::GpuStats)> {
    let start = Instant::now();
    if let Some(p) = progress {
        p.reset();
    }
    let index = Arc::new(index);
    let marker_index = Arc::new(helixscan_core::marker::MarkerIndex::new(panel));
    let file = std::fs::File::open(path)?;
    let source: Box<dyn std::io::Read + Send> = if path.extension().is_some_and(|x| x == "gz") {
        helixscan_core::fastq::gzip_reader(file)?
    } else {
        Box::new(file)
    };
    let mut reader = FastqReader::new(source, 8 * 1024 * 1024);
    let (tx, rx) =
        crossbeam_channel::bounded::<crate::compute::GpuResultBatch>((2 * threads.max(1)).max(2));
    let (local_tx, local_rx) = crossbeam_channel::unbounded();
    let mut counts = vec![[0u32; 4]; panel.variants.len()];
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            let rx = rx.clone();
            let local = local_tx.clone();
            let index = index.clone();
            let marker_index = marker_index.clone();
            scope.spawn(move || {
                let mut counts = vec![[0u32; 4]; index.variants.len()];
                let mut qc = QcLocal::default();
                let mut markers = helixscan_core::marker::MarkerLocal::default();
                let mut scanner = helixscan_core::marker::MarkerScanner::new(marker_index, simd);
                let mut previous = vec![[0u32; 4]; index.variants.len()];
                let mut delta = vec![[0u32; 4]; index.variants.len()];
                while let Ok(batch) = rx.recv() {
                    if let Some(p) = progress {
                        p.bytes.fetch_add(batch.batch.bytes, Ordering::Relaxed);
                        p.reads
                            .fetch_add(batch.batch.records.len() as u64, Ordering::Relaxed);
                    }
                    for (r, hits) in batch.batch.records.iter().zip(&batch.hits) {
                        let seq = &batch.batch.buf[r.seq_off..r.seq_off + r.seq_len];
                        let qual = &batch.batch.buf[r.qual_off..r.qual_off + r.seq_len];
                        if let Some((v, a)) = index.vote_hits(seq, qual, hits) {
                            counts[v][a.min(3) as usize] += 1;
                        }
                        qc.add(seq, qual);
                        scanner.add(seq, &mut markers);
                        if let Some(p) = progress {
                            p.bases.fetch_add(r.seq_len as u64, Ordering::Relaxed);
                        }
                    }
                    if let Some(p) = progress {
                        for ((now, old), delta) in counts.iter().zip(&previous).zip(&mut delta) {
                            for allele in 0..4 {
                                delta[allele] = now[allele] - old[allele];
                            }
                        }
                        p.update_votes(&delta);
                        previous.copy_from_slice(&counts);
                    }
                }
                let _ = local.send((counts, qc, markers));
            });
        }
        drop(rx);
        drop(local_tx);
        let producer = scope.spawn(|| -> Result<crate::GpuStats> {
            let mut batches = Vec::new();
            while let Some(b) = reader.next_batch()? {
                batches.push(b);
                if batches.len() == 4 {
                    matcher.process(&mut batches, &tx)?;
                    batches.clear();
                }
            }
            matcher.process(&mut batches, &tx)?;
            drop(tx);
            Ok(matcher.stats().clone())
        });
        let mut qc = QcLocal::default();
        let mut markers = helixscan_core::marker::MarkerLocal::default();
        for (c, q, m) in local_rx {
            for (a, b) in counts.iter_mut().flatten().zip(c.into_iter().flatten()) {
                *a += b;
            }
            qc.merge(&q);
            markers.merge(&m);
        }
        let stats = producer
            .join()
            .map_err(|_| anyhow::anyhow!("GPU reader panicked"))??;
        let result = helixscan_core::scan::finish(
            panel,
            counts,
            qc,
            markers,
            start.elapsed().as_secs_f64(),
            "GPU candidate".into(),
        );
        Ok::<_, anyhow::Error>((result, stats))
    })
}

/// Keep the explicit GPU test harness convenient while sharing the exact scan.
pub fn gpu_scan(
    path: &Path,
    panel: &Panel,
    simd: SimdLevel,
    matcher: &mut crate::GpuMatcher,
    threads: usize,
    progress: Option<&Progress>,
) -> Result<(ScanResult, crate::GpuStats)> {
    gpu_scan_with_index(
        path,
        panel,
        simd,
        matcher,
        threads,
        progress,
        PanelIndex::with_simd(panel, simd),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GpuConfig;
    use std::path::Path;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    fn panel() -> Panel {
        serde_json::from_reader(std::fs::File::open(root().join("data/panel.json")).unwrap())
            .unwrap()
    }
    fn assert_complete_parity(cpu: &ScanResult, gpu: &ScanResult) {
        assert_eq!(gpu.calls, cpu.calls);
        assert_eq!(
            serde_json::to_vec(&gpu.qc).unwrap(),
            serde_json::to_vec(&cpu.qc).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&gpu.checks).unwrap(),
            serde_json::to_vec(&cpu.checks).unwrap()
        );
        let report = |result: &ScanResult, engine: String| {
            helixscan_core::report::report(
                &panel(),
                result.calls.clone(),
                result.qc.clone(),
                engine,
                0.0,
                result.checks.clone(),
            )
        };
        // The backend descriptions are intentionally distinct operational
        // metadata. Every biological report field must still be identical.
        let report_engine = "parity comparison".to_owned();
        assert_eq!(
            serde_json::to_vec(&report(cpu, report_engine.clone())).unwrap(),
            serde_json::to_vec(&report(gpu, report_engine)).unwrap()
        );
    }
    #[test]
    fn gpu_full_scan_matches_every_result_field() {
        let panel = panel();
        let path = root().join("testdata/small.fastq");
        let cpu = helixscan_core::scan::scan_path(&path, &panel, 2, None).unwrap();
        let config = GpuConfig {
            tune: false,
            ..Default::default()
        };
        let mut matcher = match crate::GpuMatcher::new(&PanelIndex::new(&panel), config) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("GPU unavailable for full-scan test: {e}");
                return;
            }
        };
        let (gpu, _) = gpu_scan(&path, &panel, SimdLevel::Scalar, &mut matcher, 2, None).unwrap();
        assert_complete_parity(&cpu, &gpu);
    }
    #[test]
    fn forced_gpu_and_auto_fallback_are_explicit() {
        let panel = panel();
        let path = root().join("testdata/small.fastq");
        let config = GpuConfig {
            gpu_index: Some(usize::MAX),
            tune: false,
            ..Default::default()
        };
        assert!(scan_path(&path, &panel, 1, None, Backend::Gpu, config.clone()).is_err());
        let auto = scan_path(&path, &panel, 1, None, Backend::Auto, config).unwrap();
        assert!(auto.fallback_reason.is_none());
        assert!(auto.result.engine.contains("no GPU trial"));
        assert!(auto.gpu_device.is_none());
        assert_eq!(auto.result.qc.reads, 6500);
    }
    #[test]
    fn auto_uses_the_measured_cpu_policy_without_a_gpu_trial() {
        let path = root().join("testdata/small.fastq");
        let selection =
            scan_path(&path, &panel(), 2, None, Backend::Auto, Default::default()).unwrap();
        assert!(selection.result.engine.contains(AUTO_CPU_POLICY));
        assert!(selection.fallback_reason.is_none());
        assert!(selection.gpu_device.is_none());
        assert!(selection.gpu_stats.is_none());
    }
    #[test]
    fn explicit_cpu_does_not_add_auto_operational_metadata() {
        let selection = scan_path(
            &root().join("testdata/small.fastq"),
            &panel(),
            2,
            None,
            Backend::Cpu,
            Default::default(),
        )
        .unwrap();
        assert!(!selection.result.engine.contains("auto:"));
    }
}
