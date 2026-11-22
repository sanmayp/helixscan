use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub index: usize,
    pub name: String,
    pub vendor: u32,
    pub driver: String,
    pub backend: String,
    pub max_buffer_size: u64,
    pub max_storage_buffer_binding_size: u32,
    pub max_workgroup_size_x: u32,
    pub max_compute_invocations_per_workgroup: u32,
    pub max_compute_workgroups_per_dimension: u32,
}

/// Inspect installed backends without allocating GPU compute resources.
/// Test-only injection verifies that the fast policy is model-specific.
pub fn measured_slow_p5000(index: Option<usize>) -> bool {
    let adapters = match pollster::block_on(nvidia_vulkans()) {
        Ok(adapters) => adapters,
        Err(error) => {
            eprintln!("GPU adapter probe failed; retain verified auto comparison: {error}");
            return false;
        }
    };
    measured_slow_p5000_from(adapters, index)
}

fn measured_slow_p5000_from(adapters: Vec<Adapter>, index: Option<usize>) -> bool {
    let adapter = match index {
        Some(i) => adapters.iter().find(|a| a.index == i),
        None => adapters.iter().find(|a| a.vendor == 0x10de),
    };
    adapter.is_some_and(|a| a.vendor == 0x10de && a.name == "Quadro P5000")
}

/// Inspect installed backends without allocating GPU compute resources.
pub async fn adapters() -> Result<Vec<Adapter>> {
    adapters_for(wgpu::Backends::all()).await
}

/// Lightweight Vulkan metadata lookup for backend selection. No device,
/// compute pipeline, buffer allocation, dispatch or calibration is created.
pub async fn nvidia_vulkans() -> Result<Vec<Adapter>> {
    adapters_for(wgpu::Backends::VULKAN).await
}

async fn adapters_for(backends: wgpu::Backends) -> Result<Vec<Adapter>> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    let mut output = Vec::new();
    for (index, adapter) in instance
        .enumerate_adapters(backends)
        .into_iter()
        .enumerate()
    {
        let info = adapter.get_info();
        let limits = adapter.limits();
        output.push(Adapter {
            index,
            name: info.name,
            vendor: info.vendor,
            driver: format!("{} {}", info.driver, info.driver_info),
            backend: format!("{:?}", info.backend),
            max_buffer_size: limits.max_buffer_size,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_workgroup_size_x: limits.max_compute_workgroup_size_x,
            max_compute_invocations_per_workgroup: limits.max_compute_invocations_per_workgroup,
            max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_measured_p5000_uses_the_fast_cpu_policy() {
        let adapter = |index: usize, name: &str, vendor| Adapter {
            index,
            name: name.into(),
            vendor,
            driver: "test".into(),
            backend: "Vulkan".into(),
            max_buffer_size: 0,
            max_storage_buffer_binding_size: 0,
            max_workgroup_size_x: 0,
            max_compute_invocations_per_workgroup: 0,
            max_compute_workgroups_per_dimension: 0,
        };
        let adapters = vec![
            adapter(0, "other NVIDIA", 0x10de),
            adapter(1, "Quadro P5000", 0x10de),
        ];
        assert!(!measured_slow_p5000_from(adapters.clone(), None));
        assert!(measured_slow_p5000_from(adapters.clone(), Some(1)));
        assert!(!measured_slow_p5000_from(adapters, Some(usize::MAX)));
    }
}
