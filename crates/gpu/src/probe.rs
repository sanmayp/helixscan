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
pub async fn adapters() -> Result<Vec<Adapter>> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let mut output = Vec::new();
    for (index, adapter) in instance
        .enumerate_adapters(wgpu::Backends::all())
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
