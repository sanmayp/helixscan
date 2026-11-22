use anyhow::{Context, Result, bail};
use crossbeam_channel::Sender;
use helixscan_core::{
    PanelIndex,
    fastq::Batch,
    index::{Hit, ReadHit},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::mpsc, time::Instant};
use wgpu::util::DeviceExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GpuConfig {
    /// Vulkan adapter ordinal, not an nvidia-smi/CUDA device ordinal.
    pub gpu_index: Option<usize>,
    /// Explicit cap, clamped to 2 GiB; default is 25% of discovered card memory.
    pub gpu_mem_mb: Option<u64>,
    pub tune: bool,
    /// Preferred calibration/real-scan partition size, bounded by actual limits.
    pub batch_reads: usize,
    #[cfg(test)]
    pub no_bitmap: bool,
    #[cfg(test)]
    pub hit_capacity_override: Option<u32>,
    #[cfg(test)]
    pub calibration_reads: usize,
}
impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            gpu_index: None,
            gpu_mem_mb: None,
            tune: true,
            batch_reads: 40_000,
            #[cfg(test)]
            no_bitmap: false,
            #[cfg(test)]
            hit_capacity_override: None,
            #[cfg(test)]
            calibration_reads: 512,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tune {
    pub workgroup: u32,
    pub batch_reads: usize,
    pub elapsed_seconds: f64,
}
#[derive(Clone, Debug)]
pub struct GpuStats {
    pub device: String,
    pub driver: String,
    pub memory_mb: Option<u64>,
    pub workgroup: u32,
    pub memory_cap_mb: u64,
    pub bytes: u64,
    pub reads: u64,
    pub hits: u64,
    pub dispatches: u64,
    pub elapsed_seconds: f64,
    pub tune: Tune,
    pub bench_seconds: f64,
}
/// Host-side per-read hit results, with original FASTQ storage preserved.
pub struct GpuResultBatch {
    pub batch: Batch,
    pub hits: Vec<Vec<ReadHit>>,
}
type PackedReads = (Vec<u32>, Vec<u32>, Vec<u32>, u64);
struct Part {
    batch: Batch,
    reads: Vec<u32>,
    invalid_masks: Vec<u32>,
    infos: Vec<u32>,
    output: Option<wgpu::Buffer>,
    counter: Option<wgpu::Buffer>,
    hit_capacity: u32,
}
#[derive(Deserialize, Serialize)]
struct CachedTune {
    device: String,
    workgroup: u32,
    batch_reads: usize,
}
pub struct GpuMatcher {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Vec<(u32, wgpu::ComputePipeline)>,
    bitmap: wgpu::Buffer,
    keys: wgpu::Buffer,
    values: wgpu::Buffer,
    stats: GpuStats,
    batch_reads: usize,
    tune_pending: bool,
    #[cfg(test)]
    no_bitmap: bool,
    #[cfg(test)]
    hit_capacity_override: Option<u32>,
    #[cfg(test)]
    calibration_reads: usize,
    slots: u32,
    hash_bits: u32,
    max_buffer: u64,
    max_storage: u64,
    max_dispatch: u64,
}
impl GpuMatcher {
    pub fn new(index: &PanelIndex, config: GpuConfig) -> Result<Self> {
        pollster::block_on(Self::new_async(index, config))
    }
    async fn new_async(index: &PanelIndex, config: GpuConfig) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
        let adapter = match config.gpu_index {
            Some(i) => adapters.get(i),
            None => adapters.iter().find(|a| a.get_info().vendor == 0x10de),
        }
        .context("no compatible Vulkan NVIDIA adapter; use CPU")?;
        let info = adapter.get_info();
        if info.vendor != 0x10de {
            bail!("selected Vulkan adapter is not NVIDIA; use CPU");
        }
        // This is explicitly the Vulkan ordinal, not an assumed CUDA mapping.
        let ordinal = config.gpu_index.unwrap_or(0);
        let memories = crate::memory::memories();
        let discovered = memories.get(ordinal).or_else(|| memories.first()).copied();
        let limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("HelixScan core WebGPU"),
                required_features: wgpu::Features::empty(),
                required_limits: limits.clone(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .context("cannot open NVIDIA compute device; use CPU")?;
        let memory_cap_mb = crate::memory::default_cap(&memories, ordinal, config.gpu_mem_mb);
        if memory_cap_mb < 8 {
            bail!("GPU memory cap is below 8 MiB");
        }
        if config.batch_reads == 0 {
            bail!("GPU batch size must contain at least one read");
        }
        let mut pipelines = Vec::new();
        for group in [64, 128, 256] {
            if group > limits.max_compute_workgroup_size_x
                || group > limits.max_compute_invocations_per_workgroup
            {
                continue;
            }
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("HelixScan portable WGSL panel matching"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("matcher.wgsl")
                        .replace("@workgroup_size(64,", &format!("@workgroup_size({group},"))
                        .into(),
                ),
            });
            pipelines.push((
                group,
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("HelixScan panel matcher"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            ));
        }
        if pipelines.is_empty() {
            bail!("GPU cannot execute a 64-thread core compute workgroup");
        }
        let max_buffer = limits.max_buffer_size;
        let max_storage = u64::from(limits.max_storage_buffer_binding_size);
        check_storage_size(
            "panel bitmap",
            (index.bitmap.len() * 8) as u64,
            max_buffer,
            max_storage,
        )?;
        check_storage_size(
            "panel keys",
            (index.slots.len() * 8) as u64,
            max_buffer,
            max_storage,
        )?;
        check_storage_size(
            "panel allele rows",
            (index.slots.len() * 16) as u64,
            max_buffer,
            max_storage,
        )?;
        let mut keys = Vec::with_capacity(index.slots.len() * 2);
        let mut values = Vec::with_capacity(index.slots.len() * 4);
        for (stored, hits) in &index.slots {
            // CPU storage reserves zero with canonical+1. GPU uses (0,0) empty.
            let key = (*stored).saturating_sub(1);
            keys.extend([(key >> 32) as u32, key as u32]);
            let mut packed = [u32::MAX; 4];
            if hits.len() > 4 {
                bail!(
                    "panel table slot has {} allele rows; maximum is four",
                    hits.len()
                );
            }
            for (row, hit) in hits.iter().enumerate() {
                packed[row] = pack_hit(hit);
            }
            values.extend(packed);
        }
        let bitmap = storage(
            &device,
            "panel prefilter",
            bytemuck::cast_slice(&index.bitmap),
        );
        let keys_gpu = storage(&device, "panel keys", by_u32(&keys));
        let values_gpu = storage(&device, "panel values", by_u32(&values));
        let cached = config
            .tune
            .then(|| load_tune(&info.name))
            .flatten()
            .filter(|cached| {
                cached.batch_reads > 0
                    && pipelines
                        .iter()
                        .any(|(group, _)| *group == cached.workgroup)
            });
        let batch_reads = cached
            .as_ref()
            .map_or(config.batch_reads.min(262_144), |x| x.batch_reads)
            .min(262_144);
        let workgroup = cached
            .as_ref()
            .map(|x| x.workgroup)
            .unwrap_or(pipelines[0].0);
        let tune = Tune {
            workgroup,
            batch_reads,
            elapsed_seconds: 0.0,
        };
        Ok(Self {
            device,
            queue,
            pipelines,
            bitmap,
            keys: keys_gpu,
            values: values_gpu,
            stats: GpuStats {
                device: info.name,
                driver: format!("{} {}", info.driver, info.driver_info),
                memory_mb: discovered,
                workgroup,
                memory_cap_mb,
                bytes: 0,
                reads: 0,
                hits: 0,
                dispatches: 0,
                elapsed_seconds: 0.0,
                tune,
                bench_seconds: 0.0,
            },
            batch_reads,
            tune_pending: config.tune && cached.is_none(),
            #[cfg(test)]
            no_bitmap: config.no_bitmap,
            #[cfg(test)]
            hit_capacity_override: config.hit_capacity_override,
            #[cfg(test)]
            calibration_reads: config.calibration_reads,
            slots: u32::try_from(index.slots.len()).context("GPU panel slot count exceeds u32")?,
            hash_bits: u32::try_from(index.bitmap.len() * 64)
                .context("GPU panel bitmap exceeds u32")?,
            max_buffer,
            max_storage,
            max_dispatch: u64::from(limits.max_compute_workgroups_per_dimension),
        })
    }
    pub fn stats(&self) -> &GpuStats {
        &self.stats
    }
    pub fn cached_tune(&self) -> bool {
        !self.tune_pending && self.stats.tune.elapsed_seconds == 0.0
    }

    /// Measure actual dispatch/readback combinations. These hits are discarded.
    pub fn calibrate(&mut self, index: &PanelIndex) -> Result<()> {
        if !self.tune_pending {
            return Ok(());
        }
        let started = Instant::now();
        #[cfg(test)]
        let calibration_count = self.calibration_reads;
        #[cfg(not(test))]
        let calibration_count = 32_768;
        let calibration_count = calibration_count.max(128);
        let sequences = calibration_sequences(index, calibration_count);
        let mut sizes = vec![
            128,
            self.batch_reads.min(calibration_count / 2),
            self.batch_reads.min(calibration_count),
        ];
        sizes.retain(|n| *n > 0 && *n <= sequences.len());
        sizes.sort_unstable();
        sizes.dedup();
        let groups: Vec<u32> = self.pipelines.iter().map(|(group, _)| *group).collect();
        let saved_stats = self.stats.clone();
        let saved_reads = self.batch_reads;
        let mut best: Option<Tune> = None;
        for (group, reads) in groups
            .iter()
            .flat_map(|group| sizes.iter().map(move |reads| (group, reads)))
        {
            #[cfg(test)]
            if *group != 64 {
                continue;
            }
            self.stats.workgroup = *group;
            self.batch_reads = *reads;
            let mut batches = vec![batch_for_sequences(&sequences[..*reads])];
            let (tx, rx) = crossbeam_channel::unbounded();
            let candidate_start = Instant::now();
            // A failed candidate must not become the cached configuration.
            if let Err(error) = self.process(&mut batches, &tx) {
                self.stats = saved_stats.clone();
                self.batch_reads = saved_reads;
                return Err(error).context("GPU calibration dispatch failed; use CPU");
            }
            drop(tx);
            let discarded_hits: usize = rx.into_iter().map(|batch| batch.hits.len()).sum();
            if discarded_hits != *reads {
                self.stats = saved_stats.clone();
                self.batch_reads = saved_reads;
                bail!("GPU calibration returned the wrong number of reads");
            }
            let seconds = candidate_start.elapsed().as_secs_f64();
            let tune = Tune {
                workgroup: *group,
                batch_reads: *reads,
                elapsed_seconds: seconds,
            };
            if best
                .as_ref()
                .is_none_or(|best| tune.elapsed_seconds < best.elapsed_seconds)
            {
                best = Some(tune);
            }
            if started.elapsed().as_secs_f64() >= 1.0 {
                break;
            }
            #[cfg(test)]
            if *group == 64 && sizes.last().is_some_and(|last| last == reads) {
                break;
            }
        }
        let best = best.context("GPU calibration produced no valid configuration")?;
        self.batch_reads = best.batch_reads;
        self.stats = saved_stats;
        self.stats.workgroup = best.workgroup;
        self.stats.tune = best.clone();
        self.tune_pending = false;
        self.stats.bench_seconds += started.elapsed().as_secs_f64();
        if let Err(error) = save_tune(&self.stats.device, &best) {
            // A read-only home/cache must not invalidate correct GPU results.
            eprintln!("GPU tuning cache not saved: {error}");
        }
        Ok(())
    }

    pub fn process(
        &mut self,
        batches: &mut [Batch],
        sender: &Sender<GpuResultBatch>,
    ) -> Result<()> {
        let start = Instant::now();
        let mut pending = Vec::new();
        for batch in batches {
            for part in self.split_batch(batch)? {
                // Map the previous result only after submitting the next one.
                let part = self.dispatch(part)?;
                pending.push(part);
                if pending.len() == 2 {
                    self.read(pending.remove(0), sender)?;
                }
            }
        }
        for part in pending {
            self.read(part, sender)?;
        }
        self.stats.elapsed_seconds += start.elapsed().as_secs_f64();
        Ok(())
    }
    fn split_batch(&mut self, batch: &mut Batch) -> Result<Vec<Part>> {
        let mut output = Vec::new();
        let mut offset = 0;
        let mut accounted_bytes = 0;
        let total = batch.records.len();
        let min_group = self
            .pipelines
            .iter()
            .map(|(group, _)| u64::from(*group))
            .min()
            .unwrap_or(64);
        while offset < total {
            let mut end = (offset + self.batch_reads).min(total);
            let max_len = batch.records[offset..end]
                .iter()
                .map(|record| record.seq_len)
                .max()
                .unwrap_or(0);
            let windows = u64::try_from(max_len.saturating_sub(30).div_ceil(64).max(1))?;
            let invocation_limit = self.max_dispatch.saturating_mul(min_group);
            if windows > invocation_limit {
                bail!("one FASTQ read exceeds the adapter dispatch limit");
            }
            let by_dispatch = usize::try_from((invocation_limit / windows).max(1))?;
            end = end.min(offset + by_dispatch.max(1));
            loop {
                let part = Batch {
                    id: batch.id,
                    bytes: if end == total {
                        batch.bytes.saturating_sub(accounted_bytes)
                    } else {
                        batch.bytes * ((end - offset) as u64) / total as u64
                    },
                    buf: batch.buf.clone(),
                    records: batch.records[offset..end].to_vec(),
                };
                let (reads, invalid_masks, infos, upper_hits) =
                    pack_reads(&part.records, &batch.buf)?;
                match self.pack_capacity(part, reads, invalid_masks, infos, upper_hits) {
                    Ok(part) => {
                        output.push(part);
                        accounted_bytes += output.last().expect("inserted part").batch.bytes;
                        break;
                    }
                    Err(_) if end - offset > 1 => {
                        // Keep both input and worst-case output within the budget.
                        end = offset + (end - offset) / 2;
                    }
                    Err(error) => {
                        return Err(error).context("one FASTQ read cannot fit GPU resource limits");
                    }
                }
            }
            offset = end;
        }
        batch.records.clear();
        // Clear the original storage as soon as all parts own their copies.
        batch.buf = Vec::new();
        Ok(output)
    }
    fn pack_capacity(
        &self,
        batch: Batch,
        reads: Vec<u32>,
        invalid_masks: Vec<u32>,
        infos: Vec<u32>,
        upper_hits: u64,
    ) -> Result<Part> {
        let input_sizes = [
            ("packed reads", reads.len() * 4),
            ("invalid masks", invalid_masks.len() * 4),
            ("read offsets", infos.len() * 4),
        ];
        for (name, size) in input_sizes {
            check_storage_size(name, size as u64, self.max_buffer, self.max_storage)?;
        }
        let input_bytes: u64 = input_sizes.iter().map(|(_, size)| *size as u64).sum();
        let panel_bytes = self.bitmap.size() + self.keys.size() + self.values.size();
        let memory = self.stats.memory_cap_mb * 1024 * 1024;
        // Two submitted parts share the cap; panel allocations are counted once.
        let part_budget = memory.saturating_sub(panel_bytes) / 2;
        let available = part_budget
            .saturating_sub(input_bytes)
            .min(self.max_buffer.min(self.max_storage));
        if available < 32 {
            bail!("GPU memory budget cannot hold the batch and its hit buffer");
        }
        let capacity = (available / 32).min(u64::from(u32::MAX / 8)).max(1);
        if upper_hits > capacity {
            bail!("batch worst-case hits exceed its GPU memory/dispatch budget");
        }
        #[cfg(test)]
        let hit_capacity = self
            .hit_capacity_override
            .unwrap_or(upper_hits.max(1) as u32);
        #[cfg(not(test))]
        let hit_capacity = upper_hits.max(1) as u32;
        Ok(Part {
            batch,
            reads,
            invalid_masks,
            infos,
            output: None,
            counter: None,
            hit_capacity,
        })
    }
    fn dispatch(&mut self, mut part: Part) -> Result<Part> {
        let reads = u32::try_from(part.batch.records.len())?;
        let windows = part
            .batch
            .records
            .iter()
            .map(|r| u32::try_from(r.seq_len.saturating_sub(30).div_ceil(64).max(1)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .unwrap_or(1);
        let invocations = u64::from(reads) * u64::from(windows);
        let (group, pipeline) = self
            .pipelines
            .iter()
            .find(|(group, _)| *group == self.stats.workgroup)
            .context("GPU tuning refers to an invalid pipeline")?;
        let workgroups = invocations.div_ceil(u64::from(*group));
        if workgroups > self.max_dispatch {
            bail!("GPU dispatch exceeds the adapter workgroup limit");
        }
        let config = [
            self.slots,
            reads,
            windows,
            part.hit_capacity,
            self.hash_bits,
            #[cfg(test)]
            {
                u32::from(self.no_bitmap)
            },
            #[cfg(not(test))]
            {
                0
            },
        ];
        let config = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("GPU batch configuration"),
                contents: by_u32(&config),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let packed = storage(&self.device, "packed reads", by_u32(&part.reads));
        let invalid = storage(
            &self.device,
            "invalid base masks",
            by_u32(&part.invalid_masks),
        );
        let infos = storage(&self.device, "read offsets", by_u32(&part.infos));
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded GPU hit output"),
            size: u64::from(part.hit_capacity) * 32,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let counter = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("GPU hit atomic counter"),
                contents: &[0; 4],
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            });
        let entries = [
            (0, &config),
            (1, &packed),
            (2, &infos),
            (3, &invalid),
            (4, &self.bitmap),
            (5, &self.keys),
            (6, &self.values),
            (7, &output),
            (8, &counter),
        ];
        let descriptors: Vec<_> = entries
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HelixScan compute buffers"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &descriptors,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(workgroups as u32, 1, 1);
        }
        self.queue.submit([encoder.finish()]);
        // Mapping in read() provides completion; do not serialize batch n+1.
        part.reads.clear();
        part.invalid_masks.clear();
        part.infos.clear();
        part.output = Some(output);
        part.counter = Some(counter);
        Ok(part)
    }

    fn read(&mut self, part: Part, sender: &Sender<GpuResultBatch>) -> Result<()> {
        let output = part.output.context("GPU batch has no output")?;
        let counter = part.counter.context("GPU batch has no hit counter")?;
        let counter_staging = readback_buffer(&self.device, 4);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&counter, 0, &counter_staging, 0, 4);
        self.queue.submit([encoder.finish()]);
        let raw = readback(&self.device, counter_staging.slice(..4))?;
        let count = u32::from_le_bytes(raw[..4].try_into()?);
        drop(raw);
        if count > part.hit_capacity {
            bail!("GPU hit append exhausted capacity; candidate is invalid");
        }
        let bytes = u64::from(count) * 32;
        let map_len = bytes.max(32).div_ceil(8) * 8;
        let staging = readback_buffer(&self.device, map_len);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, map_len);
        self.queue.submit([encoder.finish()]);
        let raw = readback(&self.device, staging.slice(..map_len))?;
        let mut hits = vec![Vec::new(); part.batch.records.len()];
        for row in raw.as_chunks::<32>().0.iter().take(count as usize) {
            let id = word(&row[0..4]);
            let stored = word(&row[8..12]);
            let record = part
                .batch
                .records
                .get(id as usize)
                .context("GPU returned an invalid read offset")?;
            let start = word(&row[4..8]);
            if start as usize + 31 > record.seq_len
                || (stored >> 16) as u8 > 3
                || ((stored >> 24) & 0x3f) > 30
                || stored >> 31 != 0
            {
                bail!("GPU returned an invalid hit row");
            }
            hits[id as usize].push(ReadHit {
                variant: stored as u16,
                allele: (stored >> 16) as u8,
                offset: ((stored >> 24) & 0x3f) as u8,
                strand: ((stored >> 30) & 1) as u8,
                start,
            });
        }
        self.stats.hits += u64::from(count);
        self.stats.reads += part.batch.records.len() as u64;
        self.stats.bytes += part.batch.bytes;
        self.stats.dispatches += 1;
        sender.send(GpuResultBatch {
            batch: part.batch,
            hits,
        })?;
        Ok(())
    }
}

fn check_storage_size(label: &str, size: u64, max_buffer: u64, max_storage: u64) -> Result<()> {
    if size > max_buffer || size > max_storage {
        bail!(
            "{label} ({size} bytes) exceeds adapter buffer ({max_buffer}) or storage-binding ({max_storage}) limit"
        );
    }
    Ok(())
}
fn pack_reads(records: &[helixscan_core::fastq::Record], bytes: &[u8]) -> Result<PackedReads> {
    let mut words = Vec::new();
    let mut invalid_masks = Vec::new();
    let mut infos = Vec::new();
    let mut upper_hits = 0u64;
    for record in records {
        let seq = &bytes[record.seq_off..record.seq_off + record.seq_len];
        infos.push(u32::try_from(words.len()).context("GPU packed read offset overflow")?);
        infos.push(u32::try_from(seq.len()).context("GPU read length exceeds u32")?);
        infos.extend([0, 0]);
        let mut run = 0u32;
        for chunk in 0..seq.len().div_ceil(16) {
            let mut packed = 0u32;
            let mut invalid = 0u32;
            for i in 0..16 {
                if let Some(&base) = seq.get(chunk * 16 + i) {
                    let encoded = helixscan_core::encode::encode_base(base);
                    packed |= u32::from(encoded & 3) << (2 * i);
                    if encoded == 4 {
                        invalid |= 1u32 << i;
                        run = 0;
                    } else {
                        run += 1;
                        if run >= 31 {
                            // At most four allele rows per exact-table slot.
                            upper_hits = upper_hits
                                .checked_add(4)
                                .context("GPU hit bound overflow")?;
                        }
                    }
                } else {
                    invalid |= 1u32 << i;
                }
            }
            words.push(packed);
            invalid_masks.push(invalid);
        }
    }
    Ok((words, invalid_masks, infos, upper_hits))
}
fn pack_hit(hit: &Hit) -> u32 {
    debug_assert!(hit.offset <= 30);
    debug_assert!(hit.strand <= 1);
    u32::from(hit.variant)
        | (u32::from(hit.allele) << 16)
        | (u32::from(hit.offset) << 24)
        | (u32::from(hit.strand) << 30)
}
fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("u32 row"))
}
fn by_u32(values: &[u32]) -> &[u8] {
    bytemuck::cast_slice(values)
}
fn revcomp(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|base| match base | 32 {
            b'a' => b'T',
            b'c' => b'G',
            b'g' => b'C',
            b't' => b'A',
            _ => b'N',
        })
        .collect()
}
fn batch_for_sequences(seqs: &[Vec<u8>]) -> Batch {
    let mut batch = Batch {
        id: 0,
        bytes: 0,
        buf: Vec::new(),
        records: Vec::new(),
    };
    for seq in seqs {
        let seq_off = batch.buf.len();
        batch.buf.extend_from_slice(seq);
        let qual_off = batch.buf.len();
        batch.buf.extend(std::iter::repeat_n(b'I', seq.len()));
        batch.records.push(helixscan_core::fastq::Record {
            seq_off,
            seq_len: seq.len(),
            qual_off,
        });
    }
    batch.bytes = batch.buf.len() as u64;
    batch
}
fn calibration_sequences(index: &PanelIndex, count: usize) -> Vec<Vec<u8>> {
    let variants: Vec<Vec<u8>> = index
        .variants
        .iter()
        .flat_map(|variant| {
            variant
                .alts
                .iter()
                .chain(std::iter::once(&variant.ref_allele))
                .map(|allele| {
                    format!("{}{}{}", variant.flank_left, allele, variant.flank_right).into_bytes()
                })
        })
        .collect();
    let mut state = 0x1234_5678_9abc_def0u64;
    (0..count)
        .map(|i| {
            if i % 8 == 0 && !variants.is_empty() {
                let mut seq = variants[(i / 8) % variants.len()].clone();
                if i % 16 == 0 {
                    seq = revcomp(&seq);
                }
                seq
            } else {
                (0..192)
                    .map(|position| {
                        state = state
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        if position == 47 || position == 149 {
                            b'N'
                        } else {
                            b"ACGT"[(state >> 62) as usize]
                        }
                    })
                    .collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn real_index() -> PanelIndex {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let panel: helixscan_core::Panel =
            serde_json::from_reader(fs::File::open(root.join("data/panel.json")).unwrap()).unwrap();
        PanelIndex::new(&panel)
    }
    fn matcher(index: &PanelIndex, config: GpuConfig) -> Option<GpuMatcher> {
        match GpuMatcher::new(index, config) {
            Ok(x) => Some(x),
            Err(e) => {
                eprintln!("GPU unavailable for compute test: {e}");
                None
            }
        }
    }
    fn gpu_hits(matcher: &mut GpuMatcher, seqs: &[Vec<u8>]) -> Result<Vec<Vec<ReadHit>>> {
        let mut batches = vec![batch_for_sequences(seqs)];
        let (tx, rx) = crossbeam_channel::unbounded();
        matcher.process(&mut batches, &tx)?;
        drop(tx);
        Ok(rx.into_iter().flat_map(|x| x.hits).collect())
    }
    fn assert_raw_parity(index: &PanelIndex, matcher: &mut GpuMatcher, seqs: &[Vec<u8>]) {
        let gpu = gpu_hits(matcher, seqs).unwrap();
        assert_eq!(gpu.len(), seqs.len());
        for (mut gpu, seq) in gpu.into_iter().zip(seqs) {
            let mut cpu = index.read_hits(seq);
            let key = |x: &ReadHit| (x.start, x.variant, x.allele, x.offset, x.strand);
            gpu.sort_by_key(key);
            cpu.sort_by_key(key);
            assert_eq!(gpu, cpu);
        }
    }
    #[test]
    fn cpu_first_kmer_and_hash_are_covered() {
        let index = real_index();
        let read = &index.variants[0].flank_left;
        let seq = format!(
            "{read}{}T{}",
            index.variants[0].ref_allele, index.variants[0].flank_right
        );
        assert!(!index.read_hits(seq.as_bytes()).is_empty());
        let mut keys = Vec::new();
        helixscan_core::encode::scalar_keys(seq.as_bytes(), &mut keys);
        assert_eq!(keys[0].start, 0);
        assert!(helixscan_core::index::hash(keys[0].canonical) < 2_097_152);
    }
    #[test]
    fn exact_table_matches_cpu_without_prefilter() {
        let index = real_index();
        let config = GpuConfig {
            tune: false,
            no_bitmap: true,
            ..Default::default()
        };
        let Some(mut matcher) = matcher(&index, config) else {
            return;
        };
        let variant = &index.variants[0];
        let seq = format!(
            "{}{}{}",
            variant.flank_left, variant.ref_allele, variant.flank_right
        )
        .into_bytes();
        assert_raw_parity(&index, &mut matcher, &[seq]);
    }
    #[test]
    fn multi_read_long_ambiguous_reverse_and_chunks_match_cpu() {
        let index = real_index();
        let config = GpuConfig {
            tune: false,
            batch_reads: 3,
            gpu_mem_mb: Some(8),
            ..Default::default()
        };
        let Some(mut matcher) = matcher(&index, config) else {
            return;
        };
        let mut seqs = Vec::new();
        for variant in index.variants.iter().take(5) {
            for allele in variant.alts.iter().chain([&variant.ref_allele]) {
                let seq =
                    format!("{}{}{}", variant.flank_left, allele, variant.flank_right).into_bytes();
                seqs.push(seq.clone());
                seqs.push(revcomp(&seq));
                let mut ambiguous = seq.clone();
                ambiguous.resize(220, b'A');
                ambiguous[36] = b'N';
                ambiguous[97] = b'n';
                ambiguous[173] = b'N';
                seqs.push(ambiguous);
            }
        }
        seqs.push(vec![b'N'; 1_100]);
        seqs.push((0..257).map(|i| b"ACGT"[i % 4]).collect());
        assert_raw_parity(&index, &mut matcher, &seqs);
        assert!(matcher.stats.dispatches >= seqs.len() as u64 / 3);
    }
    #[test]
    fn exhausted_hit_buffer_is_an_error_not_a_fallback_success() {
        let index = real_index();
        let variant = &index.variants[0];
        let seq = format!(
            "{}{}{}",
            variant.flank_left, variant.ref_allele, variant.flank_right
        )
        .into_bytes();
        let config = GpuConfig {
            tune: false,
            no_bitmap: true,
            hit_capacity_override: Some(1),
            ..Default::default()
        };
        let Some(mut matcher) = matcher(&index, config) else {
            return;
        };
        let error = gpu_hits(&mut matcher, &[seq]).unwrap_err();
        assert!(error.to_string().contains("exhausted capacity"));
    }
}
fn storage(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    })
}
fn readback_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GPU CPU readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}
fn readback<'a>(
    device: &wgpu::Device,
    slice: wgpu::BufferSlice<'a>,
) -> Result<wgpu::BufferView<'a>> {
    let (tx, rx) = mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait)?;
    rx.recv()?.context("GPU buffer mapping failed")?;
    Ok(slice.get_mapped_range())
}
fn tune_path() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|x| PathBuf::from(x).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("helixscan/gpu-tune.json")
}
fn load_cached_tune(path: &std::path::Path, device: &str) -> Option<CachedTune> {
    let text = fs::read_to_string(path).ok()?;
    let value: CachedTune = serde_json::from_str(&text).ok()?;
    (value.device == device
        && [64, 128, 256].contains(&value.workgroup)
        && (1..=262_144).contains(&value.batch_reads))
    .then_some(value)
}
fn load_tune(device: &str) -> Option<CachedTune> {
    load_cached_tune(&tune_path(), device)
}
pub fn save_tune(device: &str, tune: &Tune) -> Result<()> {
    let path = tune_path();
    save_cached_tune(&path, device, tune)
}
fn save_cached_tune(path: &std::path::Path, device: &str, tune: &Tune) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&CachedTune {
            device: device.into(),
            workgroup: tune.workgroup,
            batch_reads: tune.batch_reads,
        })?,
    )?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tune_tests {
    use super::*;
    #[test]
    fn tuning_cache_round_trip_and_invalid_values() {
        let path = std::env::temp_dir().join(format!(
            "helixscan-gpu-tune-test-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("cache")
        ));
        let tune = Tune {
            workgroup: 128,
            batch_reads: 8192,
            elapsed_seconds: 0.012,
        };
        save_cached_tune(&path, "GPU A", &tune).unwrap();
        let cached = load_cached_tune(&path, "GPU A").unwrap();
        assert_eq!(cached.workgroup, 128);
        assert_eq!(cached.batch_reads, 8192);
        assert!(load_cached_tune(&path, "GPU B").is_none());
        fs::write(
            &path,
            r#"{"device":"GPU A","workgroup":17,"batch_reads":0}"#,
        )
        .unwrap();
        assert!(load_cached_tune(&path, "GPU A").is_none());
        let _ = fs::remove_file(path);
    }
}
