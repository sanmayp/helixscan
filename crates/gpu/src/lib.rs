//! Generic WebGPU compute for panel k-mer matching. No CUDA toolkit,
//! architecture-specific binaries, or vendor WebGPU extensions are required.

pub mod compute;
pub mod memory;
pub mod probe;
pub mod scan;

pub use compute::{GpuConfig, GpuMatcher, GpuStats, Tune};
pub use scan::{Backend, Selection, scan_path};
