# HelixScan progress

## Environment check (2026-11-21)

- Repository contains the supplied specification and no application code.
- Stable Rust: `rustc 1.100.0`, `cargo 1.100.0`; 12 logical CPUs.
- NVIDIA Quadro P5000, 16 GiB, driver 580.196.07 available through `nvidia-smi`.
- Ensembl REST responds for GRCh38 variation requests.
- Initial repository is on branch `master`.

## M0 — complete

- Created the Cargo workspace and a runtime WebGPU adapter probe.
- Observed Vulkan adapter `Quadro P5000`, NVIDIA 580.196.07; maximum compute workgroup X 1536, maximum invocations 1536, and 65535 compute workgroups per dimension. An unused OpenGL adapter was also listed; the compute backend will prefer Vulkan/NVIDIA.
- Added `.gitignore` for generated reads and build outputs.
- M0 validation: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass.

Next: M1 verified panel builder.