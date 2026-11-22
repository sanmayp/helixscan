fn main() -> anyhow::Result<()> {
    let adapters = pollster::block_on(helixscan_gpu::probe::adapters())?;
    if adapters.is_empty() {
        println!("No WebGPU adapters found; CPU fallback is available.");
    }
    for adapter in adapters {
        println!("{adapter:#?}");
    }
    Ok(())
}
